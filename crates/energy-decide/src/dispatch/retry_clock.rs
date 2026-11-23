//! Injectable time source so retry offsets are testable without sleeping.

/// Time source used by M4 fallback retry scheduling.
pub trait RetryClock: Send + Sync + 'static {
    /// Current wall instant, epoch milliseconds.
    fn now_ms(&self) -> i64;

    /// Wait between retry attempts.
    fn sleep_ms(&self, ms: u64);
}
