//! Evidence helpers for the locked recovery verdict and its authority reconcile.

use crate::authority::{
    AuthorityAnchor, AuthorityLedger, RecoveryEvidence, ResolutionStore, StartupResolution,
    StartupResolutionRecord, StorageError, journal,
};

/// Last durable resolution case, used so replacing the anchor cannot clear recovery.
pub(crate) fn locked_recovery_record(
    resolutions: &dyn ResolutionStore,
) -> Option<(u64, StartupResolutionRecord)> {
    let next = resolutions.next_id().ok()?;
    let last = (1..next).rev().find_map(|id| {
        resolutions
            .read(id)
            .ok()
            .flatten()
            .map(|record| (id, record))
    })?;
    (last.1.case == StartupResolution::RecoveryRequired).then_some(last)
}

/// Append the explicit authority reconcile verdict that clears a locked recovery.
pub(crate) fn append_reconcile_record(
    resolutions: &dyn ResolutionStore,
    at_ms: i64,
) -> Result<u64, StorageError> {
    let id = resolutions.next_id()?;
    resolutions.append(&StartupResolutionRecord {
        id,
        case: StartupResolution::Reconciled,
        at: at_ms,
        anchor_hash: journal::hash_text("authority_reconcile"),
        checkpoint_hash: journal::hash_text("reconciled"),
        journal_head_ref: 0,
    })
}

pub(crate) fn evidence(
    reason: &'static str,
    ledger: &AuthorityLedger,
    anchor: Option<&AuthorityAnchor>,
    resolutions: &dyn ResolutionStore,
    record_id: Option<u64>,
    since_ms: i64,
) -> RecoveryEvidence {
    RecoveryEvidence::new(reason, ledger, anchor, resolutions, record_id, since_ms)
}

pub(crate) fn persist_recovery(
    anchors: &dyn crate::authority::AnchorStore,
    anchor: Option<&AuthorityAnchor>,
    evidence: &RecoveryEvidence,
) -> Result<(), StorageError> {
    let mut value = anchor.cloned().unwrap_or_else(|| {
        AuthorityAnchor::new(
            evidence.anchor_generation.unwrap_or(0),
            "",
            evidence.anchor_head.unwrap_or(0),
            evidence.anchor_chain_head.unwrap_or(0),
            evidence.anchor_checkpoint.unwrap_or(0),
            evidence.record_id.unwrap_or(0),
        )
    });
    value.lock_recovery(evidence.clone());
    anchors.atomic_write(&value)
}
