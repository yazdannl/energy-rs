//! Authority-only capability required before M4's dispatch reconciliation may run.

/// Sealed proof that the authority gate permitted one reconcile operation.
#[derive(Debug)]
pub struct ReconcilePermit {
    seq: u64,
    at_ms: i64,
}

impl ReconcilePermit {
    /// Only authority code can construct this permit.
    pub(crate) fn new(seq: u64, at_ms: i64) -> Self {
        Self { seq, at_ms }
    }
    /// Gate-call sequence that granted the permit.
    pub fn seq(&self) -> u64 {
        self.seq
    }
    /// Instant the permit was granted.
    pub fn at_ms(&self) -> i64 {
        self.at_ms
    }
}
