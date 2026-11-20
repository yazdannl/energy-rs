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
    pub fault: Option<ControlFault>,
    /// Request id whose command this read confirmed or faulted, when it resolved one.
    pub resolved_request: Option<u32>,
    /// This waiter's command was cancelled or replaced by a safety hold: resolved, not faulted.
    pub superseded: bool,
}

impl Observation {
    pub(super) fn observed(
        report: Report,
        confirmed: Option<Command>,
        fault: Option<ControlFault>,
        resolved_request: Option<u32>,
    ) -> Self {
        Self {
            report,
            confirmed,
            fault,
            resolved_request,
            superseded: false,
        }
    }

    pub(super) fn superseded(report: Report) -> Self {
        Self {
            report,
            confirmed: None,
            fault: None,
            resolved_request: None,
            superseded: true,
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.fault.is_none()
    }
}
