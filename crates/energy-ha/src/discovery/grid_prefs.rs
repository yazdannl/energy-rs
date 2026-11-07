//! Grid source preferences. `from` is import, `to` is export.

use serde::Deserialize;

use super::PowerConfig;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct GridPrefs {
    /// Grid import / consumption statistic.
    #[serde(default)]
    pub stat_energy_from: Option<String>,
    /// Grid export / return statistic.
    #[serde(default)]
    pub stat_energy_to: Option<String>,
    #[serde(default)]
    pub stat_rate: Option<String>,
    #[serde(default)]
    pub power_config: Option<PowerConfig>,
    #[serde(default)]
    pub name: Option<String>,
}
