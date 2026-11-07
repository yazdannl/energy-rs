//! Progress-based watchdog.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

/// A heartbeat that only moves forward.
///
/// The old loop judged liveness by tick *start*, so a tick that spent ten seconds in four
/// sequential read-backs looked dead (survey §7 trap 10). This one is marked at the moment progress
/// actually happens -- after every successful read, after every write, after a tick completes -- and
/// the supervisor only calls it stalled after three tick intervals without a mark.
///
/// Two clocks are kept on purpose:
///
/// * the epoch of the last mark (`last_progress`, for the API and the journal) which never moves
///   backwards, and
/// * a monotonic anchor refreshed at the same instant, which is what decides a stall.
///
/// Judging by the monotonic anchor removes both failure modes of the epoch alone: a completed tick
/// that reports its own start instant cannot rewind the heartbeat, and a wall clock that jumps
/// backwards cannot hide a stall. An elapsed epoch is still honoured, so a caller that hands in an
/// explicit instant (a test, or a resumed process) keeps working.
#[derive(Debug)]
pub struct Watchdog {
    stall_after_s: i64,
    last_progress: AtomicI64,
    monotonic: Mutex<Instant>,
}

impl Watchdog {
    /// A watchdog for a given tick interval (stall bound = 3 × tick).
    pub fn new(tick_seconds: u64) -> Self {
        let tick = i64::try_from(tick_seconds).unwrap_or(i64::MAX);
        Self::with_stall_after(tick.saturating_mul(3))
    }

    /// An explicit stall bound, seconds.
    pub fn with_stall_after(stall_after_s: i64) -> Self {
        Self {
            stall_after_s: stall_after_s.max(1),
            last_progress: AtomicI64::new(0),
            monotonic: Mutex::new(Instant::now()),
        }
    }

    /// The stall bound, seconds.
    pub fn stall_after_s(&self) -> i64 {
        self.stall_after_s
    }

    /// Mark progress: the epoch only moves forward, the monotonic anchor moves to now.
    ///
    /// Both are refreshed at the point work actually completed, so a long tick that keeps reading
    /// and writing keeps the loop alive instead of looking stale the whole time.
    pub fn mark(&self, now: i64) {
        self.last_progress.fetch_max(now, Ordering::Relaxed);
        *self.lock() = Instant::now();
    }

    /// The last mark, or `None` before the first tick.
    pub fn last_progress(&self) -> Option<i64> {
        let value = self.last_progress.load(Ordering::Relaxed);
        (value > 0).then_some(value)
    }

    /// Whether no progress has been made for the stall bound, by either clock.
    pub fn stalled(&self, now: i64) -> bool {
        let Some(last) = self.last_progress() else {
            return false;
        };
        now.saturating_sub(last) > self.stall_after_s
            || self.monotonic_elapsed_s() > self.stall_after_s as f64
    }

    /// Seconds since the last mark, measured monotonically (fractional: no truncation at the bound).
    fn monotonic_elapsed_s(&self) -> f64 {
        let anchor = *self.lock();
        anchor.elapsed().as_secs_f64()
    }

    /// The anchor, ignoring a poisoned lock: a panic in a marking thread must not blind the guard.
    fn lock(&self) -> std::sync::MutexGuard<'_, Instant> {
        self.monotonic
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
