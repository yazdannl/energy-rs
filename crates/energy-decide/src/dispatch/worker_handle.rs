//! Observable lifetime of one transport worker.

use std::sync::atomic::AtomicBool;

/// A started transport worker, tracked until its `done` flag is set.
#[derive(Debug)]
pub struct WorkerHandle {
    /// Pack whose command the worker is carrying.
    pub pack_id: String,
    /// Monotonic write generation.
    pub generation: u64,
    /// Set by the worker when its transport call has returned.
    pub done: std::sync::Arc<AtomicBool>,
}
