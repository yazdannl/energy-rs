//! One pack's part of a tick: what was asked, what was allowed, what was written.

use crate::safety::Reason;

/// The audit trail of one pack in one tick.
///
/// `requested_w` is the pack's share after allocation (or its override), `intended_w` what the
/// safety ladder would allow, and `applied_w` only what the pack measured flowing. In `advisory`/`dry_run`
/// `intended_w` is the shadow ladder's answer while `written`/`applied_w` stay false/zero: the log
/// says exactly what *would* have happened, and nothing reached the hardware.
#[derive(Debug, Clone, PartialEq)]
pub struct PackReport {
    /// The pack this row belongs to.
    pub pack_id: String,
    /// Ledger id, when the pack is registered.
    pub ledger_id: Option<i64>,
    /// Allocated (or overridden) setpoint, W.
    pub requested_w: f64,
    /// What the ladder allows, W (the shadow value outside `auto`).
    pub intended_w: f64,
    /// What the pack measured flowing, W; `0.0` when nothing was verified.
    pub applied_w: f64,
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
