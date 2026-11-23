//! A learning audit record that is committed before its journal write.

/// A learning audit record that is committed before its journal write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningEvent {
    /// Outbox row id; `0` until a store that owns an outbox commits it.
    pub id: i64,
    /// Serial (or stable ledger id) of the pack that was learned.
    pub pack_id: String,
    /// Timestamp of the learning pass.
    pub at: i64,
    /// Human-readable report for the `learning.applied` journal entry.
    pub summary: String,
}

impl LearningEvent {
    /// A new, uncommitted event for one learning pass.
    pub fn new(pack_id: impl Into<String>, at: i64, summary: impl Into<String>) -> Self {
        Self {
            id: 0,
            pack_id: pack_id.into(),
            at,
            summary: summary.into(),
        }
    }
}
