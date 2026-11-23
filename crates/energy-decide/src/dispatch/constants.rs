//! Bounded timings owned by M4; one end-to-end budget per pack write.

/// End-to-end budget for apply + echo + fail-safe fallback, W-free milliseconds.
pub const PACK_IO_BUDGET_MS: u64 = 20_000;
/// Maximum allowed ramp before flow must be measured in band.
pub const RAMP_MAX_MS: u64 = 45_000;
/// Deadband below which a commanded power is a hold, W.
pub const HOLD_DEADBAND_W: f64 = 30.0;
/// Receipt window after a confirmed ramp end.
pub const RECEIPT_WINDOW_MS: u64 = 12_000;
/// Confirmed partial-flow pause duration.
pub const PARTIAL_PAUSE_MS: u64 = 120_000;
/// Retry offsets for a fail-safe hold inside the single per-pack budget.
pub const FALLBACK_RETRY_MS: [u64; 3] = [0, 2_000, 6_000];
/// No-flow fault backoff.
pub const NO_FLOW_BACKOFF_MS: u64 = 300_000;
/// Upper SoC band in which a faultLevel-0 refusal is treated as a capability shortfall.
/// Live evidence: three of four packs refused discharge at 31-34 % with device minSoc 10 %.
pub const SHORTFALL_SOC_MAX_PCT: f64 = 40.0;

/// Tolerance for a commanded power: max(30 W, 10 % of the command).
pub fn tolerance(command_w: f64) -> f64 {
    HOLD_DEADBAND_W.max(command_w.abs() * 0.10)
}

/// Whether a command is a hold under the 30 W deadband.
pub fn is_hold(command_w: f64) -> bool {
    command_w.is_finite() && command_w.abs() <= HOLD_DEADBAND_W
}
