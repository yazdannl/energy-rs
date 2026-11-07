//! One pack's physical limits, shared by planning, allocation and safety.

/// The physical limits of one pack, in the units the planner uses (kWh, W).
///
/// The same type feeds the aggregate battery spec, the per-pack allocation and the clamp ladder, so
/// a reserve floor or an efficiency cannot mean two different things on the two sides of a tick.
#[derive(Debug, Clone, PartialEq)]
pub struct PackSpec {
    /// Stable pack id (serial or ledger id).
    pub pack_id: String,
    /// Usable capacity, kWh.
    pub capacity_kwh: f64,
    /// Configured minimum SoC, percent.
    pub min_soc_pct: f64,
    /// Configured maximum SoC, percent.
    pub max_soc_pct: f64,
    /// AC charge ceiling, W.
    pub max_charge_w: f64,
    /// AC discharge ceiling, W.
    pub max_discharge_w: f64,
    /// Charge efficiency in `(0, 1]`.
    pub eta_charge: f64,
    /// Discharge efficiency in `(0, 1]`.
    pub eta_discharge: f64,
}

impl PackSpec {
    /// Why this pack cannot be planned, or `None` when it is usable.
    pub fn invalid(&self) -> Option<&'static str> {
        if self.pack_id.trim().is_empty() {
            return Some("pack id is empty");
        }
        if !self.capacity_kwh.is_finite() || self.capacity_kwh <= 0.0 {
            return Some("capacity is not a positive number");
        }
        if !self.min_soc_pct.is_finite() || !self.max_soc_pct.is_finite() {
            return Some("SoC bounds are not finite");
        }
        if self.min_soc_pct < 0.0 || self.max_soc_pct > 100.0 || self.min_soc_pct > self.max_soc_pct
        {
            return Some("SoC bounds are outside 0..=100 or inverted");
        }
        if !self.max_charge_w.is_finite()
            || !self.max_discharge_w.is_finite()
            || self.max_charge_w < 0.0
            || self.max_discharge_w < 0.0
        {
            return Some("power limits are not finite or negative");
        }
        if !self.eta_charge.is_finite()
            || !self.eta_discharge.is_finite()
            || self.eta_charge <= 0.0
            || self.eta_charge > 1.0
            || self.eta_discharge <= 0.0
            || self.eta_discharge > 1.0
        {
            return Some("efficiencies are outside 0..=1");
        }
        None
    }

    /// Lowest usable energy, applying the fleet reserve on top of the pack's own floor.
    pub fn e_min_kwh(&self, reserve_soc_pct: f64) -> f64 {
        self.capacity_kwh * self.min_soc_pct.max(reserve_soc_pct) / 100.0
    }

    /// Highest usable energy.
    pub fn e_max_kwh(&self) -> f64 {
        self.capacity_kwh * self.max_soc_pct / 100.0
    }

    /// Energy at a measured SoC.
    pub fn energy_kwh(&self, soc_pct: f64) -> f64 {
        self.capacity_kwh * soc_pct / 100.0
    }

    /// AC watts this pack can absorb or deliver over `slot_hours`, down to its own floor.
    pub fn headroom_w(
        &self,
        energy_kwh: f64,
        charging: bool,
        slot_hours: f64,
        reserve_soc_pct: f64,
    ) -> f64 {
        let hours = if slot_hours > 0.0 { slot_hours } else { 0.25 };
        if charging {
            let room = (self.e_max_kwh() - energy_kwh).max(0.0);
            let by_soc = room / (hours * self.eta_charge) * 1000.0;
            by_soc.max(0.0).min(self.max_charge_w)
        } else {
            let available = (energy_kwh - self.e_min_kwh(reserve_soc_pct)).max(0.0);
            let by_soc = available * self.eta_discharge / hours * 1000.0;
            by_soc.max(0.0).min(self.max_discharge_w)
        }
    }
}
