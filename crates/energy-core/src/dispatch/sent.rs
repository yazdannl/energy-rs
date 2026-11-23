//! The result of one transport attempt, before it becomes a `PackReport`.

use crate::dispatch::AppliedPowerReason;

/// What one send attempt produced.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // written, verified and flow_measured are different facts
pub(crate) struct Sent {
    pub(crate) written: bool,
    pub(crate) verified: bool,
    /// Whether `applied_w` came from a fresh measurement rather than being unknown.
    pub(crate) flow_measured: bool,
    pub(crate) applied_w: f64,
    /// Why the watts are unknown, when they could not be measured.
    pub(crate) applied_w_reason: Option<AppliedPowerReason>,
    pub(crate) fail_safe: bool,
    /// A write was accepted but flow will be checked on a later tick.
    pub(crate) confirmation_pending: bool,
    /// Why a safety hold took over this attempt; ownership stays with that hold.
    pub(crate) superseded: Option<String>,
    /// A newer setpoint was not sent because an earlier command is still pending.
    pub(crate) blocked_by_pending: bool,
    /// The end-to-end budget phase that timed out, when it did.
    pub(crate) timeout: Option<String>,
    pub(crate) error: Option<String>,
}

impl Sent {
    /// Nothing reached a device: no write, no receipt, no fault.
    pub(crate) const fn none() -> Self {
        Self {
            written: false,
            verified: false,
            flow_measured: false,
            applied_w: 0.0,
            applied_w_reason: None,
            fail_safe: false,
            confirmation_pending: false,
            superseded: None,
            blocked_by_pending: false,
            timeout: None,
            error: None,
        }
    }
}
