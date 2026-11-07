//! The dispatcher's heartbeat callback.

/// A progress mark the dispatcher advances after every successful device write.
///
/// The watchdog judges liveness by progress, not by tick start: a four-pack tick can spend ten
/// seconds in sequential read-backs and is still alive, while a tick that keeps failing never marks.
/// The engine implements this as its watchdog mark; a test passes nothing.
pub trait Progress: Send + Sync {
    /// Mark progress at `now` (UTC epoch seconds).
    fn mark(&self, now: i64);
}

/// Advance the heartbeat, if the caller asked for one.
pub(crate) fn advance(progress: Option<&dyn Progress>, now: i64) {
    if let Some(progress) = progress {
        progress.mark(now);
    }
}
