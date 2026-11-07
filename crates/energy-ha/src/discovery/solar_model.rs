//! Normalized solar mapping.

use super::{PowerRule, SolarPrefs};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SolarModel {
    pub energy: Option<String>,
    /// Signed W, positive = production. Multiple arrays are summed strictly.
    pub power: Option<PowerRule>,
    pub forecast_entry_ids: Vec<String>,
}

impl SolarModel {
    pub(crate) fn merge(&mut self, prefs: &SolarPrefs, rules: &mut Vec<PowerRule>) {
        if self.energy.is_none() {
            self.energy.clone_from(&prefs.stat_energy_from);
        }
        if let Some(rule) =
            PowerRule::from_source(prefs.stat_rate.as_deref(), prefs.power_config.as_ref())
        {
            rules.push(rule);
        }
        for entry in prefs
            .config_entry_solar_forecast
            .iter()
            .flatten()
            .filter(|entry| !entry.is_empty())
        {
            if !self.forecast_entry_ids.contains(entry) {
                self.forecast_entry_ids.push(entry.clone());
            }
        }
    }
}
