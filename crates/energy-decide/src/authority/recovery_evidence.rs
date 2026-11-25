//! Structured, durable evidence for the `recovery_required` resolution verdict.

use serde::{Deserialize, Serialize};

/// What failed reconciliation: which heads, which anchor and which mismatch.
///
/// This is diagnostic evidence, never an authority capability and never a device command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEvidence {
    /// Typed mismatch reason slug.
    pub reason: String,
    /// Ledger event head at resolution time.
    pub ledger_head: u64,
    /// Anchor committed head, when an anchor could be read.
    pub anchor_head: Option<u64>,
    /// Anchor pending head, when an anchor could be read.
    pub anchor_pending_head: Option<u64>,
    /// Ledger authority checkpoint.
    pub ledger_checkpoint: u64,
    /// Anchor-recorded checkpoint.
    pub anchor_checkpoint: Option<u64>,
    /// Ledger journal hash-chain head.
    pub ledger_chain_head: u64,
    /// Anchor-recorded hash-chain head.
    pub anchor_chain_head: Option<u64>,
    /// Authority generation found in the anchor.
    pub anchor_generation: Option<u64>,
    /// Startup-resolution record referenced by the anchor.
    pub resolution_ref: Option<u64>,
    /// Whether that referenced record could be read.
    pub resolution_present: Option<bool>,
    /// Diagnostic record written for this resolution.
    pub record_id: Option<u64>,
    /// Instant this verdict was first locked.
    pub since_ms: i64,
}

impl RecoveryEvidence {
    /// Build evidence with the supplied heads and mismatch reason.
    pub fn new(
        reason: &'static str,
        ledger: &crate::authority::AuthorityLedger,
        anchor: Option<&crate::authority::AuthorityAnchor>,
        resolutions: &dyn crate::authority::ResolutionStore,
        record_id: Option<u64>,
        since_ms: i64,
    ) -> Self {
        let ref_id = anchor.and_then(|value| value.last_startup_resolution_ref);
        Self {
            reason: reason.to_owned(),
            ledger_head: ledger.head_event_id(),
            anchor_head: anchor.map(|value| value.committed_head),
            anchor_pending_head: anchor.map(|value| value.pending_head),
            ledger_checkpoint: ledger.checkpoint_event_id(),
            anchor_checkpoint: anchor.map(|value| value.checkpoint_event_id),
            ledger_chain_head: ledger.chain_head(),
            anchor_chain_head: anchor.map(|value| value.journal_hash_chain_head),
            anchor_generation: anchor.map(|value| value.generation),
            resolution_ref: ref_id,
            resolution_present: ref_id.map(|id| matches!(resolutions.read(id), Ok(Some(_)))),
            record_id,
            since_ms,
        }
    }

    /// One-line explanation used by the banner, audit row and API detail.
    pub fn detail(&self) -> String {
        format!(
            "reason={} ledger_head={} anchor_head={:?} anchor_pending_head={:?} \
             ledger_checkpoint={} anchor_checkpoint={:?} chain_head={} anchor_chain_head={:?} \
             anchor_generation={:?} resolution_ref={:?} resolution_present={:?} record_id={:?}",
            self.reason,
            self.ledger_head,
            self.anchor_head,
            self.anchor_pending_head,
            self.ledger_checkpoint,
            self.anchor_checkpoint,
            self.ledger_chain_head,
            self.anchor_chain_head,
            self.anchor_generation,
            self.resolution_ref,
            self.resolution_present,
            self.record_id
        )
    }
}
