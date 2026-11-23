//! Shared logs passed to every write started in one slot.

use std::sync::{Arc, Mutex};

use crate::dispatch::{LateWrite, WorkerHandle, WorkerTerminationViolation};

/// Late-write, violation and worker-lifetime logs shared by one slot's writes.
pub struct WriteTaskContext<'a> {
    /// Late completions from superseded generations.
    pub late: &'a Arc<Mutex<Vec<LateWrite>>>,
    /// Workers that outlived the termination bound.
    pub violations: &'a Arc<Mutex<Vec<WorkerTerminationViolation>>>,
    /// All started workers and their termination flags.
    pub workers: &'a Arc<Mutex<Vec<WorkerHandle>>>,
}
