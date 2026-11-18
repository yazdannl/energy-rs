//! Normalized grid mapping.

use super::{GridPrefs, PowerRule};

/// Every grid source on an energy dashboard.
///
/// `import`/`export` stay the first source's ids: the live meter and the existing `/api/v1/site`
/// view keep using one primary id. A P1 meter can keep separate cumulative counters per tariff,
/// so `import_sources`/`export_sources` collect every configured id (in dashboard order) for the
/// measured history, which sums them per slot. Blank ids are not statistics and are skipped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridModel {
    /// Import / consumption statistic of the first (primary) grid source.
    pub import: Option<String>,
    /// Export / return statistic of the first (primary) grid source.
    pub export: Option<String>,
    /// Every grid source's import statistic, in dashboard order.
    pub import_sources: Vec<String>,
    /// Every grid source's export statistic, in dashboard order.
    pub export_sources: Vec<String>,
    /// Signed W, positive = import.
    pub power: Option<PowerRule>,
}

impl GridModel {
    pub(crate) fn merge(&mut self, prefs: &GridPrefs, rules: &mut Vec<PowerRule>) {
        if self.import.is_none() {
            self.import.clone_from(&prefs.stat_energy_from);
        }
        if let Some(id) = prefs
            .stat_energy_from
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            self.import_sources.push(id.to_owned());
        }
        if self.export.is_none() {
            self.export.clone_from(&prefs.stat_energy_to);
        }
        if let Some(id) = prefs
            .stat_energy_to
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            self.export_sources.push(id.to_owned());
        }
        if let Some(rule) =
            PowerRule::from_source(prefs.stat_rate.as_deref(), prefs.power_config.as_ref())
        {
            rules.push(rule);
        }
    }
}
