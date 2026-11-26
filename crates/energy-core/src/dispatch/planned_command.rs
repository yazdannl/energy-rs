//! One pack's command as the tick decided it, before the fleet check and the send.

use crate::allocation::ChargeSkip;
use crate::safety::Reason;

/// A pack's row for this tick, decided purely, then validated as a vector, then sent.
///
/// `command_w` is the value that would reach the device -- the ladder's answer in `auto`, the shadow
/// ladder's answer in `dry_run` -- and it is what the fleet validation rewrites when the summed
/// vector would break the site limit. `vector` marks the packs that take part in that vector
/// (reachable, not backing off) and `send` marks the ones a command is actually sent to, so a
/// `dry_run` still reports the clamped intent without pretending anything was written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Planned {
    pub(crate) pack_id: String,
    pub(crate) ledger_id: Option<i64>,
    pub(crate) measured_w: f64,
    /// Signed pack power from a fresh reading; `None` when stale or unavailable.
    pub(crate) measured_flow_w: Option<f64>,
    /// Device clock of that fresh reading, so a measured row can prove its sample.
    pub(crate) measured_at: Option<i64>,
    pub(crate) requested_w: f64,
    pub(crate) command_w: f64,
    pub(crate) reason: Reason,
    pub(crate) intended_reason: Reason,
    pub(crate) note: Option<String>,
    /// The fault that put this pack in backoff, when it is being skipped this tick.
    pub(crate) backoff_cause: Option<String>,
    /// The device-reported ceiling verdict that removed this pack from the charge split.
    pub(crate) skip: Option<ChargeSkip>,
    pub(crate) vector: bool,
    pub(crate) send: bool,
}
