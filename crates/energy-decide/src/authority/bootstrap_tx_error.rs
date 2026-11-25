//! Errors from the sealed bootstrap transaction.

/// Typed bootstrap-transaction refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapTxError {
    /// Provenance was ambiguous; operator recovery is required.
    ProvenanceAmbiguous,
    /// Token/generation pair was absent or malformed.
    TokenInvalid,
    /// Initialization event was missing or mismatched.
    EventInvalid,
    /// Event generation did not match the token generation.
    GenerationMismatch,
    /// A capability outside token/event/commit was attempted.
    CapabilityDenied,
    /// The same transaction seed was already committed.
    ReplayDenied,
}

impl BootstrapTxError {
    /// Stable typed slug.
    pub fn slug(self) -> &'static str {
        match self {
            Self::ProvenanceAmbiguous => "legacy_provenance_ambiguous",
            Self::TokenInvalid => "latch_token_invalid",
            Self::EventInvalid => "initialization_event_missing",
            Self::GenerationMismatch => "generation_mismatch",
            Self::CapabilityDenied => "bootstrap_tx_capability_denied",
            Self::ReplayDenied => "bootstrap_replay_denied",
        }
    }
}
