//! The per-tick inputs every pack shares, and the small pure helpers that read them.

use energy_battery::protection::CellLatch;
use energy_ledger::plan::PlanSlot;

use crate::dispatch::{
    HOLD_DEADBAND_W, ManualOverride, MeterReading, OverrideAction, OverrideSource, Overrides,
    PackReading,
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
    /// The fleet-level grid check could not be evaluated; every pack holds, fail-closed.
    pub(crate) grid_hold: bool,
    /// The reactive import ceiling is armed this tick.
    pub(crate) import_ceiling: bool,
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
        grid_ceiling_armed: context.import_ceiling,
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
    guard_active: bool,
) -> f64 {
    if let Some(entry) = overridden {
        return entry.action.watts();
    }
    if context.stale && !guard_active {
        0.0
    } else {
        allocated_w
    }
}

/// A cell-latch command that must bypass write backoff, and its log reason.
///
/// Both halves of the guard are safety traffic: a blocked pack needs a neutral hold, and a pack
/// below cutoff needs its emergency charge. Neither may wait for a write fault to expire, and the
/// guard cannot rely on the write path it protects. Outside `auto` nothing is written at all.
pub(crate) fn cell_safety(
    latch: Option<&CellLatch>,
    allowed_w: f64,
    mode: Authority,
) -> Option<Reason> {
    if !mode.may_write() {
        return None;
    }
    let latch = latch?;
    if latch.blocks_discharge() && allowed_w.abs() <= HOLD_DEADBAND_W {
        Some(Reason::CellVoltageGuard)
    } else if latch.forces_charge() && allowed_w > HOLD_DEADBAND_W {
        Some(Reason::CellVoltageEmergencyCharge)
    } else {
        None
    }
}

/// An armed solar-telemetry refusal is a safety hold, not an economic clamp.
pub(crate) fn telemetry_hold(decision: &SafetyDecision) -> Option<Reason> {
    let primary = decision.primary();
    matches!(
        primary,
        Reason::SolarPvTelemetryMissing | Reason::SolarLoadTelemetryMissing
    )
    .then_some(primary)
}

/// The binding reason for what this pack's row reports.
pub(crate) fn reason_for(
    context: &PackContext<'_>,
    decision: &SafetyDecision,
    overridden: Option<&ManualOverride>,
    backoff: bool,
    guarded: bool,
) -> Reason {
    let primary = decision.primary();
    // A safety bound is the binding reason even while the pack is in backoff: a cell-voltage
    // latch that refused the discharge must not be logged as a transport write failure.
    if primary != Reason::Ok {
        return primary;
    }
    let maintenance_hold = overridden.is_some_and(|entry| {
        entry.source == OverrideSource::Maintenance && entry.action == OverrideAction::Hold
    });
    if maintenance_hold {
        return Reason::MaintenanceHold;
    }
    if overridden.is_some() {
        return Reason::ManualOverride;
    }
    // The ceiling guard explains a discharge the plan did not ask for, including a stale or absent
    // plan. It outranks generic clamp reasons, but never a real safety refusal: a latched cell, an
    // empty pack or a missing meter must still say what it is.
    if guarded && !hard_safety(primary) {
        return Reason::GridImportCeiling;
    }
    if backoff {
        return Reason::FaultBackoff;
    }
    if context.stale {
        return Reason::StaleInput;
    }
    Reason::Ok
}

/// Reasons that describe a real safety refusal and must stay visible in the dispatch row.
fn hard_safety(reason: Reason) -> bool {
    matches!(
        reason,
        Reason::ModeNotAuto
            | Reason::ExitHoldSuppressed
            | Reason::BatteryUnavailable
            | Reason::StaleState
            | Reason::StateMissing
            | Reason::SocCeiling
            | Reason::SocFloor
            | Reason::GridImportLimit
            | Reason::GridExportLimit
            | Reason::GridTelemetryMissing
            | Reason::SolarPvTelemetryMissing
            | Reason::SolarLoadTelemetryMissing
            | Reason::SellFloor
            | Reason::CellVoltageGuard
            | Reason::CellVoltageEmergencyCharge
            | Reason::StaleInput
            | Reason::DriverError
            | Reason::FaultBackoff
            | Reason::WatchdogHold
            | Reason::ManualOverride
            | Reason::MaintenanceHold
            | Reason::BootHold
            | Reason::Shutdown
            | Reason::TickFailed
            | Reason::OrphanHold
            | Reason::GridImportCeiling
    )
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
        OverrideAction::Discharge { .. } => None,
    }
}
