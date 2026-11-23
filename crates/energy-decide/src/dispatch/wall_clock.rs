//! Production retry clock.

use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::dispatch::RetryClock;

/// Wall-clock implementation of the retry time source.
#[derive(Debug, Clone, Copy, Default)]
pub struct WallClock;

impl RetryClock for WallClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |value| {
                i64::try_from(value.as_millis()).unwrap_or(i64::MAX)
            })
    }

    fn sleep_ms(&self, ms: u64) {
        thread::sleep(std::time::Duration::from_millis(ms));
    }
}
