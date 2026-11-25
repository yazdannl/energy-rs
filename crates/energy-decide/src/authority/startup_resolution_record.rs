//! Append-only diagnostic record, outside the authority journal and writable in every phase.

use serde::{Deserialize, Serialize};

use crate::authority::StartupResolution;

/// One startup-resolution record at `data/startup_resolution.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupResolutionRecord {
    /// Monotonic record id referenced by the anchor.
    pub id: u64,
    /// Exactly `reconciled` or `recovery_required`.
    pub case: StartupResolution,
    /// Decision instant, epoch milliseconds.
    pub at: i64,
    /// Hash of the anchor used for the decision.
    pub anchor_hash: String,
    /// Hash of the authority checkpoint used for the decision.
    pub checkpoint_hash: String,
    /// Ledger head referenced by the decision.
    pub journal_head_ref: u64,
}
