//! Per-device dashboard consumption.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct DeviceConsumption {
    #[serde(default)]
    pub stat_consumption: Option<String>,
    #[serde(default)]
    pub stat_rate: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub included_in_stat: Option<String>,
}
