//! One pack plus the SoC it reported for the plan or the tick.

use crate::plan::PackSpec;

/// A pack and the SoC that belongs to the scenario.
///
/// Planning builds the fleet from these; allocation reuses the same shape with the live SoC, so a
/// pack's headroom is always computed from the same efficiency and reserve rule.
#[derive(Debug, Clone, PartialEq)]
pub struct PackState {
    /// The pack's physical limits.
    pub spec: PackSpec,
    /// Measured state of charge, percent.
    pub soc_pct: f64,
}

impl PackState {
    /// Stored energy at the current SoC, kWh.
    pub fn energy_kwh(&self) -> f64 {
        self.spec.energy_kwh(self.soc_pct)
    }

    /// AC watts this pack can absorb or deliver over the slot.
    pub fn headroom_w(&self, charging: bool, slot_hours: f64, reserve_soc_pct: f64) -> f64 {
        self.spec
            .headroom_w(self.energy_kwh(), charging, slot_hours, reserve_soc_pct)
    }
}
