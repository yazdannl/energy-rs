//! A transmitted command that no report has confirmed yet.

use crate::control::command::Command;
use crate::control::fault::ControlFault;

/// One command that is on the wire (or already acknowledged) and not yet echoed by a report.
///
/// It is installed *before* the write leaves, while the pacer slot is held, so a report that arrives
/// during the write already has something to verify instead of being consumed unheard. `baseline` is
/// the report that was current when the command went out: only a report strictly newer than it (see
/// [`is_fresh`](crate::control::echo::is_fresh)) may confirm or fault the command, so a duplicated or
/// pre-command report can never do either.
///
/// A fresh report that still echoes the old limits is only [`inconclusive`](Self::inconclusive):
/// the device emits reports before it applies a write, so that is a doubt, not a verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The clamped command that was sent.
    pub command: Command,
    /// Id in the write body, so a late ack or report can be matched to this attempt.
    pub request_id: u32,
    /// `(messageId, timestamp)` of the last report before the write.
    pub baseline: (u32, i64),
    /// `messageId` from the write ack, when it carried one and it was not the baseline's.
    pub acked_after: Option<u32>,
    /// An emergency neutralising hold sent while the AC path was wedged.
    pub emergency: bool,
    /// The freshest limit mismatch seen so far, held while the write gets a chance to land.
    ///
    /// Only echo faults are kept here; wedge, restart, foreign and wrong-mode reports still fail
    /// the command at once. A matching report clears the doubt and confirms; a bounded wait
    /// ([`ECHO_CONFIRM_WINDOW`](crate::control::echo::ECHO_CONFIRM_WINDOW)) turns it into a
    /// fault exactly once.
    pub inconclusive: Option<ControlFault>,
}
