//! One cost-pool reading.

/// The pool: average cost of the energy in the pack.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoolRow {
    /// Stored energy the pool describes, kWh.
    pub kwh: f64,
    /// Cost of that energy, EUR.
    pub cost_eur: f64,
    /// Last slot the pool was advanced with, UTC epoch seconds.
    pub updated_at: i64,
}

impl PoolRow {
    /// EUR per stored kWh; `None` when nothing is stored.
    pub fn basis(&self) -> Option<f64> {
        (self.kwh > 0.0 && self.cost_eur.is_finite() && self.cost_eur >= 0.0)
            .then_some(self.cost_eur / self.kwh)
    }
}
