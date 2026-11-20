//! One pack considered by the interval planner.

/// A pack's physical and historical inputs to a maintenance-charge decision.
#[derive(Debug, Clone, PartialEq)]
pub struct DuePack {
    /// Stable pack id (serial).
    pub pack_id: String,
    /// Usable capacity, kWh.
    pub capacity_kwh: f64,
    /// Measured SoC, percent.
    pub soc_pct: f64,
    /// Highest SoC the device is allowed to reach, percent.
    pub max_soc_pct: f64,
    /// AC charge efficiency, `(0, 1]`.
    pub eta_charge: f64,
    /// Per-pack AC charge ceiling, W.
    pub max_charge_w: f64,
    /// UTC epoch seconds of the last *completed* balancing dwell, if any.
    pub last_full_at: Option<i64>,
    /// Whether the operator asked for this pack now, overriding the interval.
    pub forced: bool,
}

impl DuePack {
    /// AC kWh needed to take this pack from its measured SoC to 100 %.
    pub fn ac_energy_needed_kwh(&self) -> f64 {
        if !self.capacity_kwh.is_finite()
            || self.capacity_kwh <= 0.0
            || !self.soc_pct.is_finite()
            || !self.eta_charge.is_finite()
            || self.eta_charge <= 0.0
        {
            return 0.0;
        }
        let stored = self.capacity_kwh * (100.0 - self.soc_pct.clamp(0.0, 100.0)) / 100.0;
        stored / self.eta_charge
    }

    /// Whether this pack is due: never balanced, older than the interval, or forced.
    pub fn due(&self, now: i64, interval_days: u32) -> bool {
        if self.forced {
            return true;
        }
        let interval_s = i64::from(interval_days.clamp(1, 60)) * 86_400;
        self.last_full_at
            .is_none_or(|last| now.saturating_sub(last) >= interval_s)
    }
}
