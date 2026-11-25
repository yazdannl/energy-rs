//! What an explicit hold recovery actually cleared.

/// Diagnostic returned by `SafetyLadder::recover_hold`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryStatus {
    /// The terminal hold proof was cleared.
    pub hold_proof_cleared: bool,
    /// Import protection is still armed after the proof-only recovery.
    pub import_protection_active: bool,
}
