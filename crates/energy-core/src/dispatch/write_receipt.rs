//! What a sink observed after a command.

/// The result of one command, as far as the device has proven it.
///
/// `verified` requires both the echo and measured flow in the commanded direction (a hold only
/// needs its zero-limit echo). `applied_w` is the pack's own measurement, not the clamped setpoint:
/// an accepted write whose AC path never carried power is never reported as applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WriteReceipt {
    /// The watts the pack measured flowing, W (positive charging); `0.0` when unverified.
    pub applied_w: f64,
    /// Whether the device confirmed the command with its next report.
    pub verified: bool,
    /// Real but lower-than-requested flow: the command is confirmed without claiming the setpoint.
    pub partial: bool,
    /// A safety hold replaced this command while it waited; not a device fault, no retry needed.
    pub superseded: bool,
}

impl WriteReceipt {
    /// A command the device echoed.
    pub const fn verified(applied_w: f64) -> Self {
        Self {
            applied_w,
            verified: true,
            partial: false,
            superseded: false,
        }
    }

    /// Direction-correct flow that stayed below the command: measured, not a fault.
    pub const fn partial(applied_w: f64) -> Self {
        Self {
            applied_w,
            verified: false,
            partial: true,
            superseded: false,
        }
    }

    /// A command the device accepted but has not echoed yet.
    pub const fn unverified() -> Self {
        Self {
            applied_w: 0.0,
            verified: false,
            partial: false,
            superseded: false,
        }
    }

    /// A command a safety hold deliberately superseded: resolved without a fault.
    pub const fn superseded() -> Self {
        Self {
            applied_w: 0.0,
            verified: false,
            partial: false,
            superseded: true,
        }
    }
}
