//! What a sink observed after a command.

/// The result of one command, as far as the device has proven it.
///
/// `verified` is the echo check, not the HTTP result: `energy-battery` only confirms a write when
/// the next report echoes its limit within tolerance. An accepted but unconfirmed write still
/// returns `written = true` through [`PackReport`](crate::dispatch::PackReport) and stays latched,
/// because the device may be executing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WriteReceipt {
    /// The watts the device echoed, W (positive charging); `0.0` for a hold.
    pub applied_w: f64,
    /// Whether the device confirmed the command with its next report.
    pub verified: bool,
}

impl WriteReceipt {
    /// A command the device echoed.
    pub const fn verified(applied_w: f64) -> Self {
        Self {
            applied_w,
            verified: true,
        }
    }

    /// A command the device accepted but has not echoed yet.
    pub const fn unverified() -> Self {
        Self {
            applied_w: 0.0,
            verified: false,
        }
    }
}
