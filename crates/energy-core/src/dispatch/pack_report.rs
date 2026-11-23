//! One pack's part of a tick: what was asked, what was allowed, what was written.

use crate::dispatch::AppliedPowerReason;
use crate::safety::Reason;

/// The audit trail of one pack in one tick.
///
/// `requested_w` is the pack's share after allocation (or its override), `intended_w` what the
/// safety ladder would allow, and `applied_w` only what the pack measured flowing. In `advisory`/`dry_run`
/// `intended_w` is the shadow ladder's answer while `written`/`applied_w` stay false/zero: the log
/// says exactly what *would* have happened, and nothing reached the hardware. A suppressed tick
/// can carry measured flow: `written` says nothing reached the device this tick, `verified` says no
/// echo was seen, and `flow_measured` says whether `applied_w` is a real reading at all.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // written, verified and flow_measured are different facts
pub struct PackReport {
    /// The pack this row belongs to.
    pub pack_id: String,
    /// Ledger id, when the pack is registered.
    pub ledger_id: Option<i64>,
    /// Allocated (or overridden) setpoint, W.
    pub requested_w: f64,
    /// What the ladder allows, W (the shadow value outside `auto`).
    pub intended_w: f64,
    /// What the pack measured flowing, W; meaningful only when `flow_measured` is true.
    pub applied_w: f64,
    /// Whether `applied_w` came from a fresh reading; false means unknown, not zero delivery.
    pub flow_measured: bool,
    /// Why the applied watts are unknown; `None` when a measurement exists or was not expected.
    pub applied_w_reason: Option<AppliedPowerReason>,
    /// The binding reason for what happened.
    pub reason: Reason,
    /// The ladder's own reason for `intended_w`.
    pub intended_reason: Reason,
    /// Whether a command reached the device.
    pub written: bool,
    /// Whether the next reports echoed the command *and* proved real power streaming.
    pub verified: bool,
    /// Whether this row is the one-shot fail-safe hold after a fault.
    pub fail_safe: bool,
    /// Why a safety hold replaced this write; ownership was transferred, not failed or cleared.
    pub superseded: Option<String>,
    /// The transport/echo error, when there was one.
    pub error: Option<String>,
    /// Why this row is what it is, in words the dispatch log can show (maintenance, balance
    /// current); `None` when the reason slug is the whole story.
    pub note: Option<String>,
}
