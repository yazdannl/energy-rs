//! Result of the fail-safe hold inside the original write budget.

use crate::dispatch::WriteAck;

/// What the bounded fail-safe hold achieved.
#[derive(Debug, Clone, PartialEq)]
pub struct FallbackResult {
    /// Final transport answer, if a write completed.
    pub ack: Option<WriteAck>,
    /// Number of attempts made.
    pub attempts: usize,
    /// Scheduled offsets, in milliseconds, actually used.
    pub offsets_ms: Vec<u64>,
    /// Elapsed milliseconds of the shared budget.
    pub elapsed_ms: u64,
}
