//! One auditable authority decision: who, why, when and what the gate answered.

use crate::authority::{AuthorityPhase, AuthorityState, EntryPoint, GateOutcome, WriteClass};

/// Append-only audit row mirrored to the ledger by wiring.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    /// Monotonic decision number; also the gate-call count.
    pub seq: u64,
    /// Decision instant, epoch milliseconds.
    pub at_ms: i64,
    /// Actor supplied by the caller.
    pub actor: String,
    /// Entry point that asked.
    pub entry: EntryPoint,
    /// Requested write class.
    pub class: WriteClass,
    /// Effective operational state at decision time.
    pub state: AuthorityState,
    /// Effective authority phase at decision time.
    pub phase: AuthorityPhase,
    /// The gate's answer.
    pub outcome: GateOutcome,
    /// Why the caller asked.
    pub why: String,
    /// Typed detail slug or durable evidence text.
    pub detail: String,
    /// Structured recovery evidence when the refusal is `authority_recovery_required`.
    pub evidence: Option<crate::authority::RecoveryEvidence>,
}

impl AuditEvent {
    /// True when this row records a refusal.
    pub fn refused(&self) -> bool {
        !self.outcome.authorized()
    }
}
