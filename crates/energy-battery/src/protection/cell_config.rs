//! Per-pack cell-voltage thresholds and the power an emergency charge may use.

/// The guard's configuration for one pack.
///
/// The thresholds are per chemistry; the power limits are per pack, which is why the guard carries
/// none of this and every evaluation takes the config it should judge against. `cutoff_v` below
/// `resume_v` is required for the middle band to exist; a misconfigured pair still fails safe,
/// because the `v < cutoff` test runs first and only `v >= cutoff` can clear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellConfig {
    /// Below this cell voltage the pack must not discharge and must be charged.
    pub cutoff_v: f64,
    /// At or above this voltage the latch clears.
    pub resume_v: f64,
    /// A reading older than this many seconds is stale and keeps the previous latch.
    pub stale_after_s: f64,
    /// The pack's own charge ceiling, W.
    pub max_charge_w: u32,
    /// The emergency charge power, W; 0 means "use the full charge ceiling".
    pub emergency_power_w: u32,
}

impl CellConfig {
    /// A config with the defaults the old add-on shipped: 3.00 V cutoff, 3.18 V resume, 180 s stale.
    pub fn new(
        cutoff_v: f64,
        resume_v: f64,
        stale_after_s: f64,
        max_charge_w: u32,
        emergency_power_w: u32,
    ) -> Self {
        Self {
            cutoff_v,
            resume_v,
            stale_after_s,
            max_charge_w,
            emergency_power_w,
        }
    }

    /// `min(max_charge, emergency_power)`; an emergency power of 0 means the full ceiling.
    ///
    /// The minimum, not the emergency value: a pack with a 600 W charge ceiling must not be asked
    /// for 1200 W just because the emergency setting is high.
    pub fn force_charge_w(&self) -> u32 {
        if self.emergency_power_w == 0 {
            self.max_charge_w
        } else {
            self.max_charge_w.min(self.emergency_power_w)
        }
    }
}

impl Default for CellConfig {
    fn default() -> Self {
        Self {
            cutoff_v: 3.00,
            resume_v: 3.18,
            stale_after_s: 180.0,
            max_charge_w: 1000,
            emergency_power_w: 800,
        }
    }
}
