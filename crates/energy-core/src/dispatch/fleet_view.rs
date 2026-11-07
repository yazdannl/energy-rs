//! One consistent read of every pack.

use crate::dispatch::PackReading;

/// The fleet as one tick sees it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FleetView {
    /// One entry per configured pack.
    pub packs: Vec<PackReading>,
}

impl FleetView {
    /// A view over the given readings.
    pub fn new(packs: Vec<PackReading>) -> Self {
        Self { packs }
    }

    /// One pack by id.
    pub fn pack(&self, pack_id: &str) -> Option<&PackReading> {
        self.packs.iter().find(|pack| pack.pack_id() == pack_id)
    }

    /// Summed configured capacity, kWh.
    pub fn capacity_kwh(&self) -> f64 {
        self.packs.iter().map(|pack| pack.spec.capacity_kwh).sum()
    }

    /// Summed measured AC power, W (positive charging).
    pub fn power_w(&self) -> f64 {
        self.packs.iter().map(|pack| pack.power_w).sum()
    }

    /// Summed charge ceiling, W.
    pub fn max_charge_w(&self) -> f64 {
        self.packs
            .iter()
            .map(|pack| pack.spec.max_charge_w)
            .sum::<f64>()
    }

    /// The pack ids, in view order.
    pub fn pack_ids(&self) -> Vec<&str> {
        self.packs.iter().map(PackReading::pack_id).collect()
    }

    /// Capacity-weighted SoC of the packs that reported one.
    pub fn soc_pct(&self) -> Option<f64> {
        let mut energy = 0.0;
        let mut capacity = 0.0;
        for pack in &self.packs {
            let Some(soc) = pack.soc_pct else {
                continue;
            };
            if pack.spec.capacity_kwh <= 0.0 {
                continue;
            }
            energy += soc * pack.spec.capacity_kwh;
            capacity += pack.spec.capacity_kwh;
        }
        (capacity > 0.0).then(|| energy / capacity)
    }
}
