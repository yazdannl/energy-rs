//! One configured history source for a grid category.

#[derive(Debug, Clone, PartialEq)]
pub struct GridSource {
    /// Stable identifier used in notices and plan quality.
    pub source_id: String,
    /// Category key: grid_import, grid_export or pv for required sources.
    pub category: &'static str,
    /// Whether a hole in this source can refuse the plan.
    pub required: bool,
    /// Whether the source could be read at all.
    pub available: bool,
    /// One optional measurement per covered slot, in horizon order.
    pub slots: Vec<Option<f64>>,
}
