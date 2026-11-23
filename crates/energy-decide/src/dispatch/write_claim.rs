//! Single-claim arbitration for one write generation.

use std::sync::{Condvar, Mutex};
use std::time::Instant;

/// Shared claim; exactly one of worker, watchdog or collector owns an attempt's fate.
#[derive(Debug, Default)]
pub struct Claim {
    done: Mutex<bool>,
    changed: Condvar,
}

impl Claim {
    pub fn claim(&self) -> bool {
        let mut done = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *done {
            return false;
        }
        *done = true;
        self.changed.notify_all();
        true
    }

    pub fn wait(&self, deadline: Instant) {
        let mut done = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*done {
            let now = Instant::now();
            if now >= deadline {
                return;
            }
            let (guard, result) = self
                .changed
                .wait_timeout(done, deadline.saturating_duration_since(now))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            done = guard;
            if result.timed_out() {
                return;
            }
        }
    }
}
