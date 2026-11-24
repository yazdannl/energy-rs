//! Shared context for one slot's bounded, arbiter-driven attempts.

use std::sync::{Arc, Mutex};

use crate::dispatch::{DriveBoundEvent, LateWrite, PackIo, RetryClock};

/// Everything a write attempt needs from the dispatcher, without long argument lists.
pub struct DispatchContext<'a> {
    /// Shared pack transport.
    pub io: &'a Arc<dyn PackIo>,
    /// Retry clock for fail-safe offsets.
    pub clock: &'a dyn RetryClock,
    /// Late completions from superseded generations.
    pub late: &'a Arc<Mutex<Vec<LateWrite>>>,
    /// Driver-loop bound violations.
    pub bound: &'a Arc<Mutex<Vec<DriveBoundEvent>>>,
    /// Hard transport return-time bound.
    pub transport_bound_ms: u64,
    /// Arbiter poll bound for one tick.
    pub max_polls: u64,
    /// Per-pack end-to-end budget.
    pub budget_ms: u64,
    /// Slot time in epoch milliseconds.
    pub now_ms: i64,
}
