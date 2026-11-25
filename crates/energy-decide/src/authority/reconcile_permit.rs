//! Authority-only capability required before a reconcile may run.

/// Sealed proof that the authority gate permitted one reconcile operation.
#[derive(Debug)]
pub struct ReconcilePermit {
    seq: u64,
    at_ms: i64,
    actor: String,
    why: String,
}

impl ReconcilePermit {
    /// Only authority code can construct this permit.
    pub(crate) fn new(seq: u64, at_ms: i64, actor: &str, why: &str) -> Self {
        Self {
            seq,
            at_ms,
            actor: actor.to_owned(),
            why: why.to_owned(),
        }
    }
    /// Gate-call sequence that granted the permit.
    pub fn seq(&self) -> u64 {
        self.seq
    }
    /// Instant the permit was granted.
    pub fn at_ms(&self) -> i64 {
        self.at_ms
    }
    /// Audited actor who received the capability.
    pub fn actor(&self) -> &str {
        &self.actor
    }
    /// Audited reason for the capability.
    pub fn why(&self) -> &str {
        &self.why
    }
}
