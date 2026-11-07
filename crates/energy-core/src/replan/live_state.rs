//! The live fleet state the SoC deviation trigger is judged against.

/// Stored energy and capacity at the moment of the check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveState {
    /// Summed measured energy, kWh.
    pub energy_kwh: f64,
    /// Summed configured capacity, kWh.
    pub capacity_kwh: f64,
}
