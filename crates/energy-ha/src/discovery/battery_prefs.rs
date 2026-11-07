//! Battery source preferences. `from` is discharge, `to` is charge.

use serde::Deserialize;

use super::PowerConfig;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct BatteryPrefs {
    /// Discharge (energy out of the battery).
    #[serde(default)]
    pub stat_energy_from: Option<String>,
    /// Charge (energy into the battery).
    #[serde(default)]
    pub stat_energy_to: Option<String>,
    #[serde(default)]
    pub stat_soc: Option<String>,
    /// Usable energy in kWh; used only as the SoC weight.
    #[serde(default)]
    pub capacity: Option<f64>,
    #[serde(default)]
    pub stat_rate: Option<String>,
    #[serde(default)]
    pub power_config: Option<PowerConfig>,
    #[serde(default)]
    pub name: Option<String>,
}
