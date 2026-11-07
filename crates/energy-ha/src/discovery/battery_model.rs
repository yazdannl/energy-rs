//! Normalized battery mapping.

use super::{BatteryPrefs, PowerRule};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatteryModel {
    pub name: Option<String>,
    /// Discharge statistic (`stat_energy_from`).
    pub discharge: Option<String>,
    /// Charge statistic (`stat_energy_to`).
    pub charge: Option<String>,
    pub soc: Option<String>,
    /// Usable kWh, used as the SoC weight.
    pub capacity: Option<f64>,
    /// Signed W, positive = discharge.
    pub power: Option<PowerRule>,
}

impl BatteryModel {
    pub(crate) fn from_prefs(prefs: &BatteryPrefs) -> Self {
        Self {
            name: prefs.name.clone(),
            discharge: prefs.stat_energy_from.clone(),
            charge: prefs.stat_energy_to.clone(),
            soc: prefs.stat_soc.clone(),
            capacity: prefs.capacity,
            power: PowerRule::from_source(prefs.stat_rate.as_deref(), prefs.power_config.as_ref()),
        }
    }
}
