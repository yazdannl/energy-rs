//! Shared context for one arbiter-driven batch of writes.

use std::sync::{Arc, Mutex};

use crate::dispatch::{DriveBoundEvent, LateWrite, PackIo, ReleaseQueue, RetryClock};

/// Transport, logs and loop bound used by the single write driver.
pub struct DriveContext<'a> {
    /// Shared transport.
    pub io: &'a Arc<dyn PackIo>,
    /// Non-blocking release queue; transport release never runs on the arbiter.
    pub release: &'a ReleaseQueue,
    /// Retry clock for fail-safe offsets.
    pub clock: &'a dyn RetryClock,
    /// Late completions from superseded generations.
    pub late: &'a Arc<Mutex<Vec<LateWrite>>>,
    /// Driver-loop bound violations.
    pub bound: &'a Arc<Mutex<Vec<DriveBoundEvent>>>,
    /// Maximum arbiter polls in one tick; guards loops, never restarts a future.
    pub max_polls: u64,
}
