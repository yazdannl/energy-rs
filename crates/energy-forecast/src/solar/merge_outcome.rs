//! The merged values and the per-source report.

use super::merge_row::MergeRow;

/// The merged values plus one report row per source.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    /// One kW value per grid slot, floored at zero.
    pub values: Vec<f64>,
    /// One row per configured source, in configured order.
    pub report: Vec<MergeRow>,
}
