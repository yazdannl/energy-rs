//! What one poll learned.

use crate::control::command::Command;
use crate::control::fault::ControlFault;
use crate::report::Report;

/// A fresh report plus the two things the controller derives from it: whether the pending command
/// echoed, and whether the device is faulted. A fault is data, not an error, so a wedged device
/// still yields a readable report for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub report: Report,
    pub confirmed: Option<Command>,
    pub fault: Option<ControlFault>,
    /// The device could not be read back while waiting for an echo. The write was accepted, so a
    /// read miss is not a control fault; the caller must keep the command latched instead.
    pub read_error: Option<String>,
}

impl Observation {
    pub fn is_healthy(&self) -> bool {
        self.fault.is_none() && self.read_error.is_none()
    }
}
