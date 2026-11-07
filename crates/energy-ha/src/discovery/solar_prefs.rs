//! Solar source preferences.

use serde::Deserialize;

use super::PowerConfig;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SolarPrefs {
    #[serde(default)]
    pub stat_energy_from: Option<String>,
    #[serde(default)]
    pub stat_rate: Option<String>,
    #[serde(default)]
    pub power_config: Option<PowerConfig>,
    /// Opaque config-entry ids for `energy/solar_forecast`.
    #[serde(default)]
    pub config_entry_solar_forecast: Option<Vec<String>>,
    #[serde(default)]
    pub name: Option<String>,
}
