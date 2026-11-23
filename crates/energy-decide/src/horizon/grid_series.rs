//! Summed grid series plus the evidence behind the sum.

use crate::horizon::GridSourceCoverage;

#[derive(Debug, Clone, PartialEq)]
pub struct GridSeries {
    /// One summed value per covered slot; required categories always have full data.
    pub values: Vec<f64>,
    /// Coverage for every configured source, including empty ones.
    pub coverage: Vec<GridSourceCoverage>,
    /// Source ids skipped because a redundant source covers their category.
    pub partial_sources: Vec<String>,
}
