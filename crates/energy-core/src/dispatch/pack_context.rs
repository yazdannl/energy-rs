//! The per-tick inputs every pack shares, and the small pure helpers that read them.

use energy_battery::protection::CellLatch;
use energy_ledger::plan::PlanSlot;

use crate::dispatch::{
    ManualOverride, MeterReading, OverrideAction, OverrideSource, Overrides, PackReading, Progress,
};
use crate::safety::{Authority, Measured, Reason, SafetyDecision, SafetyLimits};

/// The per-tick inputs every pack shares.
pub(crate) struct PackContext<'a> {
    pub(crate) now: i64,
    pub(crate) mode: Authority,
    pub(crate) stale: bool,
    pub(crate) slot: Option<&'a PlanSlot>,
    pub(crate) limits: &'a SafetyLimits,
    pub(crate) meter: &'a MeterReading,
    pub(crate) fleet_power_w: f64,
    pub(crate) latches: &'a [CellLatch],
    pub(crate) overrides: &'a Overrides,
    pub(crate) progress: Option<&'a dyn Progress>,
    /// The fleet-level grid check could not be evaluated; every pack holds, fail-closed.
    pub(crate) grid_hold: bool,
}

/// The measurements one command is judged against.
pub(crate) fn measured(context: &PackContext<'_>, reading: &PackReading) -> Measured {
    Measured {
        energy_kwh: reading.energy_kwh.or_else(|| {
            reading
                .soc_pct
                .filter(|soc| soc.is_finite())
                .map(|soc| reading.spec.energy_kwh(soc))
        }),
        soc_pct: reading.soc_pct,
        power_w: reading.power_w,
        fleet_power_w: context.fleet_power_w,
        grid_w: context.meter.grid_w,
        load_kw: context.meter.load_kw,
        pv_kw: context.meter.pv_kw,
        available: reading.available,
        stale: reading.stale,
    }
}

/// The packs a latched cell guard forbids discharging.
pub(crate) fn blocked_discharge(latches: &[CellLatch]) -> Vec<&str> {
    latches
        .iter()
        .filter(|latch| latch.blocks_discharge())
        .map(|latch| latch.pack_id.as_str())
        .collect()
}

/// The setpoint this pack is asked for: an override first, a hold while stale, the split otherwise.
pub(crate) fn requested_watts(
    context: &PackContext<'_>,
    overridden: Option<&ManualOverride>,
    allocated_w: f64,
) -> f64 {
    if let Some(entry) = overridden {
        return entry.action.watts();
    }
    if context.stale { 0.0 } else { allocated_w }
}

/// The binding reason for what this pack's row reports.
pub(crate) fn reason_for(
    context: &PackContext<'_>,
    decision: &SafetyDecision,
    overridden: Option<&ManualOverride>,
    backoff: bool,
) -> Reason {
    if backoff {
        return Reason::FaultBackoff;
    }
    let maintenance_hold = overridden.is_some_and(|entry| {
        entry.source == OverrideSource::Maintenance && entry.action == OverrideAction::Hold
    });
    match decision.primary() {
        Reason::Ok if maintenance_hold => Reason::MaintenanceHold,
        Reason::Ok if overridden.is_some() => Reason::ManualOverride,
        Reason::Ok if context.stale => Reason::StaleInput,
        other => other,
    }
}

/// The human note that belongs in the dispatch log for this pack's override.
pub(crate) fn note_for(overridden: Option<&ManualOverride>) -> Option<String> {
    let entry = overridden?;
    if entry.source != OverrideSource::Maintenance {
        return None;
    }
    match entry.action {
        OverrideAction::Hold | OverrideAction::Off => Some(
            "full-charge maintenance: neutral hold at the top of the charge; the packs draw a \
             small balance current while the BMS equalises, so the setpoint is 0 W, not an exit hold"
                .to_owned(),
        ),
        OverrideAction::Charge { .. } => {
            Some("full-charge maintenance: charging to the top balanced target".to_owned())
        }
    }
}
