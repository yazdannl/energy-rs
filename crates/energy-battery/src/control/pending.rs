//! A transmitted command that no report has confirmed yet.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::command::Command;
use crate::control::fault::ControlFault;

/// One command that is on the wire (or already acknowledged) and not yet echoed by a report.
///
/// It is installed *before* the write leaves, while the pacer slot is held, so a report that arrives
/// during the write already has something to verify instead of being consumed unheard. A report can
/// only speak about a command when it is strictly newer than the report that was current when the
/// command went out, comparing unwrapped report counters (see
/// [`is_fresh`](crate::control::echo::is_fresh)); a duplicated or pre-command report can never do
/// either. `sent_at` bounds the command's lifetime: a safety hold preempts it after
/// [`SAFETY_HOLD_WINDOW`](crate::control::controller::SAFETY_HOLD_WINDOW), and the readback waits at
/// most [`ECHO_CONFIRM_WINDOW`](crate::control::echo::ECHO_CONFIRM_WINDOW).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The clamped command that was sent.
    pub command: Command,
    /// Id in the write body, so a waiter and an expiry can only touch this attempt.
    pub request_id: u32,
    /// `(messageId, timestamp)` of the last report before the write, for diagnostics.
    pub baseline: (u32, i64),
    /// Unwrapped counter of that baseline report; wraps are added as reports arrive.
    pub baseline_sequence: u64,
    /// Newest unwrapped counter already consumed for this command (baseline or a doubt report).
    pub last_seen_sequence: u64,
    /// `messageId` from the write ack, when it carried one and it was not the baseline's.
    pub acked_after: Option<u32>,
    /// Unwrapped counter of the ack, derived from the baseline when the ring distance was safe.
    pub acked_sequence: Option<u64>,
    /// Monotonic instant the command was installed, used by the safety-hold preemption bound.
    pub sent_at: Instant,
    /// An emergency neutralising hold sent while the AC path was wedged.
    pub emergency: bool,
    /// The freshest limit mismatch seen so far, held while the write gets a chance to land.
    ///
    /// Only echo faults are kept here; wedge, restart, foreign and wrong-mode reports still fail
    /// the command at once. A matching report clears the doubt and confirms; the bounded wait
    /// turns it into a fault exactly once.
    pub inconclusive: Option<ControlFault>,
}

impl Pending {
    /// Seconds between the baseline report and now, for diagnostics in timeout faults.
    pub fn age(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.sent_at)
    }
}
