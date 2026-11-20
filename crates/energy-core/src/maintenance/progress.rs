//! Dwell observation: stamps the history only after the balance window completed.

use energy_battery::protection::full_charge::{FullChargeAssignment, TOLERANCE_PCT};

use crate::dispatch::{FleetView, PackReading};
use crate::maintenance::MaintenancePort;
use crate::plan::Plan;

use std::collections::HashSet;

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
    let mut outcomes: Vec<DwellOutcome> = plan
        .maintenance
        .iter()
        .filter(|assignment| now >= assignment.charge_end)
        .filter_map(|assignment| observe(port, assignment, view, now))
        .collect();
    let assigned: HashSet<&str> = plan
        .maintenance
        .iter()
        .map(|assignment| assignment.pack_id.as_str())
        .collect();
    for pack in port.history().pending_dwells() {
        if assigned.contains(pack.as_str())
            || outcomes.iter().any(|outcome| outcome.pack_id == pack)
        {
            continue;
        }
        // The plan that started this dwell is gone. Never let a stale row block a later attempt.
        outcomes.push(abandon_named(port, &pack, "assignment_lost"));
    }
    outcomes
}

/// Clear every unconfirmed dwell after a restart and return one outcome per pack.
///
/// The stored start is deleted in the same operation that is journalled, so no later plan can
/// finish a dwell whose beginning it never observed.
pub fn abort_pending(port: &MaintenancePort, reason: &'static str) -> Vec<DwellOutcome> {
    port.history()
        .pending_dwells()
        .into_iter()
        .map(|pack_id| abandon_named(port, &pack_id, reason))
        .collect()
}

fn abandon_named(port: &MaintenancePort, pack_id: &str, reason: &'static str) -> DwellOutcome {
    if let Err(error) = port.history().abort_dwell(pack_id) {
        tracing::warn!(%error, pack = pack_id, "stale dwell could not be cleared");
    }
    DwellOutcome {
        pack_id: pack_id.to_owned(),
        completed: false,
        reason,
    }
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
