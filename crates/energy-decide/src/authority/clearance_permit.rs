//! Proof that one logged clearance entered `clearing` before any device read.

/// Single-use clearance identity; the barrier id is `clearance_id:generation`.
#[derive(Debug)]
pub struct ClearancePermit {
    clearance_id: String,
    generation: u64,
    started_at_ms: i64,
    seq: u64,
}

impl ClearancePermit {
    /// Only authority code can construct a permit.
    pub(crate) fn new(clearance_id: String, generation: u64, started_at_ms: i64, seq: u64) -> Self {
        Self {
            clearance_id,
            generation,
            started_at_ms,
            seq,
        }
    }
    /// Operator clearance id.
    pub fn clearance_id(&self) -> &str {
        &self.clearance_id
    }
    /// Per-pack barrier generation recorded before evidence reads.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Durable clearance-entry instant.
    pub fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }
    /// Audit sequence of the entry event.
    pub fn seq(&self) -> u64 {
        self.seq
    }
    /// Identity used by every post-barrier pack report.
    pub fn barrier_id(&self) -> String {
        format!("{}:{}", self.clearance_id, self.generation)
    }
}
