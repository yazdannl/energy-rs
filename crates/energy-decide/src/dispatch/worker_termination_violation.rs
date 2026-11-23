//! A transport worker that outlived the worker-termination bound.

/// Recorded when a worker has not returned by `2 x budget` after cancellation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerTerminationViolation {
    /// Pack whose worker did not terminate in time.
    pub pack_id: String,
    /// Monotonic generation of the write.
    pub generation: u64,
    /// Enforced bound in milliseconds.
    pub bound_ms: u64,
    /// Workers still outstanding when the bound was checked.
    pub outstanding: usize,
}
