//! The six physical battery-aggregate values every solve needs.

/// The six physical battery-aggregate values every solve needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatterySpec {
    /// Lowest usable stored energy, kWh.
    pub e_min_kwh: f64,
    /// Highest usable stored energy, kWh.
    pub e_max_kwh: f64,
    /// Maximum charge power, kW.
    pub p_charge_kw: f64,
    /// Maximum discharge power, kW.
    pub p_discharge_kw: f64,
    /// Charge efficiency (0..=1).
    pub eta_charge: f64,
    /// Discharge efficiency (0..=1).
    pub eta_discharge: f64,
}
