//! The result of a write that the device acknowledged.

use crate::control::command::Command;
use crate::transport::WriteAck;

/// A command the device accepted with `success: true, code: 200`, plus the clamped values that were
/// actually sent. Acceptance is not confirmation: the echo lands in the next report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub command: Command,
    pub ack: WriteAck,
}

impl Applied {
    /// The limit this command will be verified against.
    pub fn expected_w(&self) -> Option<u32> {
        self.command.watts()
    }
}
