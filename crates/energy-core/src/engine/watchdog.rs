//! Progress-based watchdog.

use std::sync::atomic::{AtomicI64, Ordering};

/// A heartbeat that only moves forward.
///
/// The old loop judged liveness by tick *start*, so a tick that spent ten seconds in four
/// sequential read-backs looked dead (survey §7 trap 10). This one is marked at tick start, after
/// every read and after every write, and the supervisor only calls it stalled after three tick
/// intervals without a mark. Marks are epoch seconds and never move backwards, so a completed tick
/// reporting its start moment cannot rewind the heartbeat.
#[derive(Debug)]
pub struct Watchdog {
    stall_after_s: i64,
    last_progress: AtomicI64,
}

impl Watchdog {
    /// A watchdog for a given tick interval (stall bound = 3 × tick).
    pub fn new(tick_seconds: u64) -> Self {
        let tick = i64::try_from(tick_seconds).unwrap_or(i64::MAX);
        Self {
            stall_after_s: tick.saturating_mul(3),
            last_progress: AtomicI64::new(0),
        }
    }

    /// An explicit stall bound, seconds.
    pub fn with_stall_after(stall_after_s: i64) -> Self {
        Self {
            stall_after_s: stall_after_s.max(1),
            last_progress: AtomicI64::new(0),
        }
    }

    /// The stall bound, seconds.
    pub fn stall_after_s(&self) -> i64 {
        self.stall_after_s
    }

    /// Mark progress; the value only moves forward.
    pub fn mark(&self, now: i64) {
        self.last_progress.fetch_max(now, Ordering::Relaxed);
    }

    /// The last mark, or `None` before the first tick.
    pub fn last_progress(&self) -> Option<i64> {
        let value = self.last_progress.load(Ordering::Relaxed);
        (value > 0).then_some(value)
    }

    /// Whether no progress has been made for the stall bound.
    pub fn stalled(&self, now: i64) -> bool {
        self.last_progress()
            .is_some_and(|last| now.saturating_sub(last) > self.stall_after_s)
    }
}
