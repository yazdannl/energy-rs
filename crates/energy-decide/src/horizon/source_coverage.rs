//! Per-source coverage record that reaches plan quality even on refusal.

use crate::horizon::{EmptyReason, SourceOutcome};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridSourceCoverage {
    /// Source identifier.
    pub source_id: String,
    /// Whether the source is required.
    pub required: bool,
    /// Slots with a usable value.
    pub slots_present: usize,
    /// Slots without a usable value.
    pub slots_missing: usize,
    /// All-empty versus hole, when the source is not full.
    pub empty_reason: Option<EmptyReason>,
    /// Typed result for this source.
    pub outcome: SourceOutcome,
}
