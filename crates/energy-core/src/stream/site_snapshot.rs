//! The discovered site: what HA gave us and what the engine can therefore do.

use crate::stream::SiteSource;

/// The resolved energy dashboard and the capabilities that follow from it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SiteSnapshot {
    /// False when `energy/get_prefs` reported no dashboard; every source is then empty.
    pub configured: bool,
    /// The configured grid/solar/load entities, in dashboard order.
    pub sources: Vec<SiteSource>,
    /// Stable capability slugs (`battery_control`, `savings`, …) the UI may rely on.
    pub capabilities: Vec<String>,
}
