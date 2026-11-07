//! Normalized grid mapping.

use super::{GridPrefs, PowerRule};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridModel {
    /// Import / consumption statistic.
    pub import: Option<String>,
    /// Export / return statistic.
    pub export: Option<String>,
    /// Signed W, positive = import.
    pub power: Option<PowerRule>,
}

impl GridModel {
    pub(crate) fn merge(&mut self, prefs: &GridPrefs, rules: &mut Vec<PowerRule>) {
        if self.import.is_none() {
            self.import.clone_from(&prefs.stat_energy_from);
        }
        if self.export.is_none() {
            self.export.clone_from(&prefs.stat_energy_to);
        }
        if let Some(rule) =
            PowerRule::from_source(prefs.stat_rate.as_deref(), prefs.power_config.as_ref())
        {
            rules.push(rule);
        }
    }
}
