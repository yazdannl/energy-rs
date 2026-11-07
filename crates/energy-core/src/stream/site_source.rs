//! One discovered energy source.

/// A grid, solar or load entry from the HA energy dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteSource {
    /// `grid`, `solar` or `load`.
    pub kind: String,
    /// The HA entity id that carries the source's energy counter.
    pub entity_id: String,
    /// The dashboard label, when the dashboard sets one.
    pub label: Option<String>,
}
