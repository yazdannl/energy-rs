//! The live measurements one command is judged against.

/// What the dispatcher measured for this pack and this tick.
///
/// The grid meter already contains the batteries' current action, so `fleet_power_w` is the
/// correction back to the battery-idle base; the ladder never uses `grid_w` raw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    /// Stored energy, kWh; preferred over `soc_pct`.
    pub energy_kwh: Option<f64>,
    /// Measured SoC, percent, when the driver reports it.
    pub soc_pct: Option<f64>,
    /// This pack's last applied setpoint, W (the ramp baseline).
    pub power_w: f64,
    /// Summed current battery AC power, W (positive charging).
    pub fleet_power_w: f64,
    /// Grid meter, W (positive import).
    pub grid_w: Option<f64>,
    /// Measured house load, kW.
    pub load_kw: Option<f64>,
    /// Measured PV production, kW.
    pub pv_kw: Option<f64>,
    /// Whether the driver says the pack is reachable.
    pub available: bool,
    /// Whether the driver marks the measurement stale.
    pub stale: bool,
}

impl Default for Measured {
    fn default() -> Self {
        Self {
            energy_kwh: None,
            soc_pct: None,
            power_w: 0.0,
            fleet_power_w: 0.0,
            grid_w: None,
            load_kw: None,
            pv_kw: None,
            available: true,
            stale: false,
        }
    }
}
