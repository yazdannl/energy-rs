//! One report row per configured source.

use super::state::SourceState;

/// One report row per configured source.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeRow {
    /// Source name.
    pub name: String,
    /// Merge weight.
    pub weight: f64,
    /// State as reported (a non-empty series with `failed` status still contributes).
    pub state: SourceState,
    /// Provider note.
    pub note: String,
    /// Number of series points.
    pub points: usize,
}
