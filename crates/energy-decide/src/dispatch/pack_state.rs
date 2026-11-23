//! M4-owned per-pack bookkeeping; safety and authority decisions stay in their ports.

use crate::dispatch::PendingConfirmation;

/// Confirmation, uncertainty, backoff and pause state for one pack.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackState {
    /// Outstanding confirmation, when flow has not yet verified the command.
    pub pending: Option<PendingConfirmation>,
    /// Whether an uncertain write blocks ordinary retries.
    pub uncertain: bool,
    /// Typed reason for the uncertainty.
    pub uncertain_reason: Option<&'static str>,
    /// End of ordinary fault backoff, epoch milliseconds.
    pub backoff_until_ms: i64,
    /// Typed reason for ordinary backoff.
    pub backoff_reason: Option<&'static str>,
    /// End of the confirmed partial-flow pause, epoch milliseconds.
    pub pause_until_ms: i64,
    /// Target that created the current pause, for interruption on change.
    pub pause_target_w: Option<f64>,
    /// Number of persistent partial retries already made.
    pub partial_retries: usize,
    /// Consecutive fresh neutral reports while no command is standing.
    pub idle_reports: usize,
}

impl PackState {
    /// Whether ordinary traffic is currently deferred.
    pub fn ordinary_blocked(&self, now_ms: i64) -> bool {
        self.uncertain || now_ms < self.backoff_until_ms || now_ms < self.pause_until_ms
    }
}
