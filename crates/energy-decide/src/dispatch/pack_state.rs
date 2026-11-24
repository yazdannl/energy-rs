//! M4-owned per-pack bookkeeping; safety and authority decisions stay in their ports.

use crate::dispatch::PendingConfirmation;

/// Confirmation, uncertainty, backoff and pause state for one pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PackState {
    /// Outstanding confirmation, when flow has not yet verified the command.
    pub pending: Option<PendingConfirmation>,
    /// Whether an uncertain write blocks ordinary retries.
    pub uncertain: bool,
    /// Typed reason for the uncertainty.
    pub uncertain_reason: Option<&'static str>,
    /// How truth was re-established after uncertainty: verified_hold or read_back.
    pub uncertainty_cleared_by: Option<&'static str>,
    /// True once every transport future carrying this pack's command is gone.
    pub transport_gone: bool,
    /// Latched after a hard transport-contract violation; cleared only by reconcile.
    pub transport_contract_violated: bool,
    /// Slot time of the last write that could physically apply, epoch milliseconds.
    pub last_apply_attempt_ms: i64,
    /// Slot time when that write's future was joined, epoch milliseconds.
    pub joined_at_ms: i64,
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
    /// Device timestamp of the last accepted neutralising hold, epoch milliseconds.
    pub last_neutral_ack_at_ms: i64,
    /// Whether the consumer declared in-order physical application for this pack.
    pub ordered_application: Option<bool>,
    /// Monotonic generation of the latest write; late older writes are ignored.
    pub generation: u64,
    /// Last command identity tracked for this pack, accepted or not.
    pub last_command: Option<crate::dispatch::PackCommand>,
}

impl Default for PackState {
    fn default() -> Self {
        Self {
            pending: None,
            uncertain: false,
            uncertain_reason: None,
            uncertainty_cleared_by: None,
            transport_gone: true,
            transport_contract_violated: false,
            last_apply_attempt_ms: 0,
            joined_at_ms: 0,
            backoff_until_ms: 0,
            backoff_reason: None,
            pause_until_ms: 0,
            pause_target_w: None,
            partial_retries: 0,
            idle_reports: 0,
            last_neutral_ack_at_ms: 0,
            ordered_application: None,
            generation: 0,
            last_command: None,
        }
    }
}

impl PackState {
    /// Whether ordinary traffic is currently deferred.
    pub fn ordinary_blocked(&self, now_ms: i64) -> bool {
        self.uncertain
            || self.transport_contract_violated
            || now_ms < self.backoff_until_ms
            || now_ms < self.pause_until_ms
    }
}
