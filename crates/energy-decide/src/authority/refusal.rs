//! Typed reasons for every refused write; no refusal is a silent no-op.

use crate::authority::AuthorityState;

/// Every reason the gate or a narrower authority operation can refuse a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityRefusal {
    /// Generic device/control-plane denial through the single gate.
    WriteDeniedByAuthorityGate,
    /// No write of any device class while latched.
    DryRunLatched,
    /// Clearing permits only fenced safety-lane neutralisation.
    ClearingAllowsNeutralisationOnly,
    /// A neutralisation write needs a fresh, unseen fence.
    FreshFenceRequired,
    /// Domain persistence is runtime-only outside `auto`.
    PersistenceDenied(AuthorityState),
    /// No valid token; only the sealed bootstrap transaction may write.
    AuthorityUninitialized,
    /// Missing, malformed or unexplained token.
    LatchTokenInvalid,
    /// Explicit operator recovery state; nothing but the diagnostic record is writable.
    AuthorityRecoveryRequired,
    /// Ordinary append cannot write, lower or replace the authority checkpoint.
    CheckpointWriteDenied,
    /// Bootstrap transaction has no device/queue/settings/append capability.
    BootstrapTxCapabilityDenied,
    /// Queued work was an intent, not an authorisation, when it reached execution.
    IntentNotAuthorized,
    /// Journal range is truncated, unordered or hash-invalid.
    JournalIncomplete,
    /// Anchor does not match the ledger or its referenced resolution record.
    AnchorMismatch,
    /// Startup-resolution append or readback failed.
    ResolutionRecordCorrupt,
    /// Pending anchor reference could not be installed atomically.
    AnchorRefPendingFailed,
    /// Pending anchor reference could not be promoted atomically.
    AnchorRefPromotionFailed,
    /// Storage was full while writing startup resolution.
    ResolutionRecordStorageFull,
    /// Read-only filesystem while writing startup resolution.
    ResolutionRecordReadOnly,
    /// File fsync failed while writing startup resolution.
    ResolutionRecordFsyncFailed,
    /// Parent-directory fsync failed while writing startup resolution.
    ResolutionDirFsyncFailed,
    /// Clearance command was missing actor, time, clearance id or a health check.
    ClearanceAuthorizationInvalid,
    /// Operational `dry_run` requests are rejected, never entered.
    DryRunNotOperational,
    /// Only a logged clearance command can change authority state.
    AuthorityTransitionDenied,
    /// Reconcile is authority-only; ordinary callers cannot invoke it.
    ReconcileDenied,
    /// Missing or ambiguous legacy provenance.
    LegacyProvenanceAmbiguous,
    /// A replayed bootstrap commit was presented as a new transaction.
    BootstrapReplayDenied,
}

impl AuthorityRefusal {
    /// Stable, machine-readable slug used by M4 rows and API payloads.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::WriteDeniedByAuthorityGate => "write_denied_by_authority_gate",
            Self::DryRunLatched => "dry_run_latched",
            Self::ClearingAllowsNeutralisationOnly => "clearing_allows_neutralisation_only",
            Self::FreshFenceRequired => "fresh_fence_required",
            Self::PersistenceDenied(_) => "persistence_denied",
            Self::AuthorityUninitialized => "authority_uninitialized",
            Self::LatchTokenInvalid => "latch_token_invalid",
            Self::AuthorityRecoveryRequired => "authority_recovery_required",
            Self::CheckpointWriteDenied => "checkpoint_write_denied",
            Self::BootstrapTxCapabilityDenied => "bootstrap_tx_capability_denied",
            Self::IntentNotAuthorized => "intent_not_authorized",
            Self::JournalIncomplete => "journal_incomplete",
            Self::AnchorMismatch => "anchor_mismatch",
            Self::ResolutionRecordCorrupt => "resolution_record_corrupt",
            Self::AnchorRefPendingFailed => "anchor_ref_pending_failed",
            Self::AnchorRefPromotionFailed => "anchor_ref_promotion_failed",
            Self::ResolutionRecordStorageFull => "resolution_record_storage_full",
            Self::ResolutionRecordReadOnly => "resolution_record_read_only",
            Self::ResolutionRecordFsyncFailed => "resolution_record_fsync_failed",
            Self::ResolutionDirFsyncFailed => "resolution_dir_fsync_failed",
            Self::ClearanceAuthorizationInvalid => "clearance_authorization_invalid",
            Self::DryRunNotOperational => "dry_run_not_operational",
            Self::AuthorityTransitionDenied => "authority_transition_denied",
            Self::ReconcileDenied => "reconcile_denied",
            Self::LegacyProvenanceAmbiguous => "legacy_provenance_ambiguous",
            Self::BootstrapReplayDenied => "bootstrap_replay_denied",
        }
    }
}
