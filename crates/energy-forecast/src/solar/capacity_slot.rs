//! One capacity-source slot: power, the attenuation applied and its uncertainty.

/// One capacity-source slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapacitySlot {
    /// AC power, kW, before the bias correction.
    pub pv_kw: f64,
    /// Attenuation that was applied.
    pub attenuation: f64,
    /// Uncertainty band, percent.
    pub uncertainty_pct: f64,
}
