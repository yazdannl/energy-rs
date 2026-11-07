//! Normalized per-device consumption mapping.

use super::{DeviceConsumption, PowerRule};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceModel {
    pub name: Option<String>,
    pub consumption: Option<String>,
    /// Positive W.
    pub power: Option<PowerRule>,
    pub included_in_stat: Option<String>,
}

impl DeviceModel {
    pub(crate) fn from_prefs(prefs: &DeviceConsumption) -> Self {
        Self {
            name: prefs.name.clone(),
            consumption: prefs.stat_consumption.clone(),
            power: PowerRule::from_source(prefs.stat_rate.as_deref(), None),
            included_in_stat: prefs.included_in_stat.clone(),
        }
    }
}
