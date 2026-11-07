//! Stable reason slugs for every bound the ladder can apply.

use std::fmt;

/// The reason a command was clamped or blocked.
///
/// These are the old add-on's slugs (`Reason` in `models.py`) for the clauses this phase ports; a
/// caller can compare on the enum and log [`Reason::as_str`] without parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// Nothing bound the request.
    Ok,
    /// The mode is advisory or dry-run; nothing may be written.
    ModeNotAuto,
    /// An exit/boot hold was requested outside `auto`; a dry run writes nothing.
    ExitHoldSuppressed,
    /// The pack is unavailable.
    BatteryUnavailable,
    /// The pack's measurement is stale.
    StaleState,
    /// The pack has neither energy nor a usable SoC.
    StateMissing,
    /// The pack is at its SoC ceiling.
    SocCeiling,
    /// The pack is at its SoC floor (including the reserve).
    SocFloor,
    /// The plan's own SoC trajectory for this slot bound the setpoint.
    PlanTarget,
    /// The pack or fleet power ceiling bound the setpoint.
    PowerClamp,
    /// The grid import cap bound the setpoint.
    GridImportLimit,
    /// The grid export cap bound the setpoint.
    GridExportLimit,
    /// Neither a meter nor a forecast base exists to evaluate the grid cap; hold.
    GridTelemetryMissing,
    /// The per-pack per-tick ramp bound the setpoint.
    RampLimit,
    /// The cost-basis sell floor refused the planned discharge.
    SellFloor,
    /// The cell-voltage latch blocks discharge.
    CellVoltageGuard,
    /// The cell-voltage latch forces an emergency charge.
    CellVoltageEmergencyCharge,
}

impl Reason {
    /// Stable slug.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ModeNotAuto => "mode_not_auto",
            Self::ExitHoldSuppressed => "exit_hold_suppressed",
            Self::BatteryUnavailable => "battery_unavailable",
            Self::StaleState => "stale_state",
            Self::StateMissing => "state_missing",
            Self::SocCeiling => "soc_ceiling",
            Self::SocFloor => "soc_floor",
            Self::PlanTarget => "plan_soc_target",
            Self::PowerClamp => "power_clamp",
            Self::GridImportLimit => "grid_import_limit",
            Self::GridExportLimit => "grid_export_limit",
            Self::GridTelemetryMissing => "grid_telemetry_missing",
            Self::RampLimit => "ramp_limit",
            Self::SellFloor => "sell_floor",
            Self::CellVoltageGuard => "cell_voltage_guard",
            Self::CellVoltageEmergencyCharge => "cell_voltage_emergency_charge",
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
