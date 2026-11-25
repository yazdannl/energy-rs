//! Narrow initialization/recovery event: no command, queue or settings payload exists.

use crate::authority::BootstrapReason;

/// The only event the sealed bootstrap transaction is allowed to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationOrRecoveryEvent {
    reason: BootstrapReason,
    generation: u64,
    previous_mode: Option<String>,
    source_release: String,
    at_ms: i64,
    nonce: u64,
}

impl InitializationOrRecoveryEvent {
    /// Build an audited event for one bootstrap transaction.
    pub fn new(
        reason: BootstrapReason,
        generation: u64,
        previous_mode: Option<String>,
        source_release: String,
        at_ms: i64,
        nonce: u64,
    ) -> Self {
        Self {
            reason,
            generation,
            previous_mode,
            source_release,
            at_ms,
            nonce,
        }
    }

    /// Audited reason.
    pub fn reason(&self) -> BootstrapReason {
        self.reason
    }
    /// Generation explained by this event.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Exact previous mode found, including `missing` as `None`.
    pub fn previous_mode(&self) -> Option<&str> {
        self.previous_mode.as_deref()
    }
    /// Source release recorded for a legacy ledger.
    pub fn source_release(&self) -> &str {
        &self.source_release
    }
    /// Event instant.
    pub fn at_ms(&self) -> i64 {
        self.at_ms
    }
    /// Single-use transaction nonce, recorded in the ledger to prevent replay.
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
}
