//! What one poll learned.

use crate::control::command::Command;
use crate::control::fault::ControlFault;
use crate::report::Report;

/// A fresh report plus what the controller derived from it: which request (if any) this read
/// resolved, whether it echoed, and whether the device faulted. A fault is data, not an error, so a
/// wedged device still yields a readable report for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub report: Report,
    pub confirmed: Option<Command>,
    /// A direction change confirmed with real but lower-than-requested flow: no fault, no claim
    /// that the requested wattage was achieved. The command stays owned by the controller.
    pub partial: Option<Command>,
    pub fault: Option<ControlFault>,
    /// Request id whose command this read confirmed or faulted, when it resolved one.
    pub resolved_request: Option<u32>,
    /// This waiter's command was cancelled or replaced by a safety hold: resolved, not faulted.
    pub superseded: bool,
    /// The device could not be read back while waiting for an echo. The write was accepted, so a
    /// read miss is not a control fault; the caller must keep the command latched instead.
    pub read_error: Option<String>,
}

impl Observation {
    pub(super) fn observed(
        report: Report,
        confirmed: Option<Command>,
        partial: Option<Command>,
        fault: Option<ControlFault>,
        resolved_request: Option<u32>,
    ) -> Self {
        Self {
            report,
            confirmed,
            partial,
            fault,
            resolved_request,
            superseded: false,
            read_error: None,
        }
    }

    pub(super) fn superseded(report: Report) -> Self {
        Self {
            report,
            confirmed: None,
            partial: None,
            fault: None,
            resolved_request: None,
            superseded: true,
            read_error: None,
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.fault.is_none() && self.read_error.is_none()
    }
}
