//! One device's result inside a fan-out.

use crate::control::applied::Applied;
use crate::control::command::Command;
use crate::error::BatteryError;

/// A command sent to one device and what came back.
#[derive(Debug)]
pub struct DeviceOutcome {
    pub sn: String,
    pub command: Command,
    pub result: Result<Applied, BatteryError>,
}

impl DeviceOutcome {
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }

    pub fn applied(&self) -> Option<&Applied> {
        self.result.as_ref().ok()
    }

    pub fn error(&self) -> Option<&BatteryError> {
        self.result.as_ref().err()
    }
}
