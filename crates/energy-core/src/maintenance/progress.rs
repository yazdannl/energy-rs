//! Dwell observation: stamps the history only after the balance window completed.

use energy_battery::protection::full_charge::{FullChargeAssignment, TOLERANCE_PCT};

use crate::dispatch::{FleetView, PackReading};
use crate::maintenance::MaintenancePort;
use crate::plan::Plan;

/// Grace after the planned dwell for a device that answered late but stayed at the target.
const DWELL_GRACE_S: i64 = 2 * 3600;

/// One dwell transition worth journalling.
#[derive(Debug, Clone, PartialEq)]
pub struct DwellOutcome {
    /// Pack whose dwell changed.
    pub pack_id: String,
    /// Whether `last_full_charge_at` was written.
    pub completed: bool,
    /// Why a dwell was abandoned, when it was.
    pub reason: &'static str,
}

/// Inspect every outstanding assignment once per tick.
pub fn progress(
    port: &MaintenancePort,
    plan: &Plan,
    view: &FleetView,
    now: i64,
) -> Vec<DwellOutcome> {
    plan.maintenance
        .iter()
        .filter(|assignment| now >= assignment.charge_end)
        .filter_map(|assignment| observe(port, assignment, view, now))
        .collect()
}

/// One assignment's observation, or `None` while it is still balancing normally.
fn observe(
    port: &MaintenancePort,
    assignment: &FullChargeAssignment,
    view: &FleetView,
    now: i64,
) -> Option<DwellOutcome> {
    let Some(pack) = view
        .packs
        .iter()
        .find(|pack| pack.pack_id() == assignment.pack_id)
    else {
        return Some(abandon(port, assignment, "pack_unavailable"));
    };
    if !pack.usable() || !reached(pack, assignment.target_soc_pct) {
        return Some(abandon(port, assignment, "target_lost"));
    }
    balance(port, assignment, now)
}

/// Start the dwell when the pack arrives, stamp it when the configured hold elapsed.
fn balance(
    port: &MaintenancePort,
    assignment: &FullChargeAssignment,
    now: i64,
) -> Option<DwellOutcome> {
    match port.history().dwell_started_at(&assignment.pack_id) {
        Some(started) => finish(port, assignment, started, now),
        None => start(port, assignment, now),
    }
}

/// The first at-target tick starts the dwell; it is not a completed balance.
fn start(
    port: &MaintenancePort,
    assignment: &FullChargeAssignment,
    now: i64,
) -> Option<DwellOutcome> {
    if now > assignment.dwell_until + DWELL_GRACE_S {
        return Some(abandon(port, assignment, "dwell_expired"));
    }
    if let Err(error) = port.history().begin_dwell(&assignment.pack_id, now) {
        tracing::warn!(%error, pack = %assignment.pack_id, "dwell start could not be saved");
    }
    None
}

/// Only the end of a continuous hold may write `last_full_charge_at`.
fn finish(
    port: &MaintenancePort,
    assignment: &FullChargeAssignment,
    started: i64,
    now: i64,
) -> Option<DwellOutcome> {
    let dwell_s = i64::from(port.settings().dwell_minutes()) * 60;
    if now.saturating_sub(started) < dwell_s {
        return None;
    }
    match port.history().finish_dwell(&assignment.pack_id, now) {
        Ok(()) => Some(DwellOutcome {
            pack_id: assignment.pack_id.clone(),
            completed: true,
            reason: "balanced",
        }),
        Err(error) => {
            tracing::warn!(%error, pack = %assignment.pack_id, "completed dwell could not be saved");
            None
        }
    }
}

/// A dwell that cannot be trusted: drop its start, but never touch the last-full stamp.
fn abandon(
    port: &MaintenancePort,
    assignment: &FullChargeAssignment,
    reason: &'static str,
) -> DwellOutcome {
    if let Err(error) = port.history().abort_dwell(&assignment.pack_id) {
        tracing::warn!(%error, pack = %assignment.pack_id, "dwell abort could not be saved");
    }
    DwellOutcome {
        pack_id: assignment.pack_id.clone(),
        completed: false,
        reason,
    }
}

/// Whether the device proved it arrived at the target.
fn reached(pack: &PackReading, target: f64) -> bool {
    pack.soc_pct
        .is_some_and(|soc| soc.is_finite() && soc >= target - TOLERANCE_PCT)
}
