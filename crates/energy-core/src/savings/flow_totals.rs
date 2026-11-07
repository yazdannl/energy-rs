//! Measured energy sums over a day or a stretch of days.

use super::SlotFlow;

/// Measured energy totals in kWh. A missing measurement contributes nothing (never a zero-filled
/// guess); a day's coverage says how much of it was really there.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlowTotals {
    pub import_kwh: f64,
    pub export_kwh: f64,
    pub pv_kwh: f64,
    pub charge_kwh: f64,
    pub discharge_kwh: f64,
}

impl FlowTotals {
    /// Add every present measurement of one slot.
    pub fn add(&mut self, slot: &SlotFlow) {
        self.import_kwh += slot.import_kwh.unwrap_or(0.0);
        self.export_kwh += slot.export_kwh.unwrap_or(0.0);
        self.pv_kwh += slot.pv_kwh.unwrap_or(0.0);
        self.charge_kwh += slot.charge_kwh.unwrap_or(0.0);
        self.discharge_kwh += slot.discharge_kwh.unwrap_or(0.0);
    }
}
