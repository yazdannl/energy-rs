//! Retry clock that records sleeps instead of waiting.

use std::sync::{Arc, Mutex};

use energy_decide::dispatch::RetryClock;

/// Retry clock that records sleeps instead of waiting.
#[derive(Debug, Default, Clone)]
pub struct ImmediateClock {
    sleeps: Arc<Mutex<Vec<u64>>>,
}

impl ImmediateClock {
    /// Sleep durations requested by fallback retries.
    pub fn sleeps(&self) -> Vec<u64> {
        self.sleeps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl RetryClock for ImmediateClock {
    fn now_ms(&self) -> i64 {
        0
    }

    fn sleep_ms(&self, ms: u64) {
        self.sleeps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(ms);
    }
}
