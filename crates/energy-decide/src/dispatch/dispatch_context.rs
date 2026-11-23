//! Shared transport context for one slot's bounded attempts.

use std::sync::{Arc, Mutex};

use crate::dispatch::{LateWrite, PackIo, RetryClock, WorkerHandle, WorkerTerminationViolation};

/// Everything a write attempt needs from the dispatcher, without long argument lists.
pub struct DispatchContext<'a> {
    /// Shared pack transport.
    pub io: &'a Arc<dyn PackIo>,
    /// Retry clock for fail-safe offsets.
    pub clock: &'a dyn RetryClock,
    /// Late completions from superseded generations.
    pub late: &'a Arc<Mutex<Vec<LateWrite>>>,
    /// Workers that outlived the termination bound.
    pub violations: &'a Arc<Mutex<Vec<WorkerTerminationViolation>>>,
    /// All started workers, with their termination flags.
    pub workers: &'a Arc<Mutex<Vec<WorkerHandle>>>,
    /// Per-pack end-to-end budget.
    pub budget_ms: u64,
    /// Slot time in epoch milliseconds.
    pub now_ms: i64,
}
