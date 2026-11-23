//! What a sink observed after a command.

/// The result of one command, as far as the device has proven it.
///
/// `verified` is the echo check only: the device accepted the command. `flow_measured` is the
/// independent question — did a fresh sample actually carry power in the commanded direction (or
/// show idle for a hold)? An echo without a fresh sample is never turned into applied watts.
// The command-echo flags are independent evidence, not a state machine; the measurement verdict
// below decides which watts may be published.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WriteReceipt {
    /// The watts the fresh sample measured flowing, W (positive charging); `0.0` when unmeasured.
    pub applied_w: f64,
    /// Whether the device confirmed the command with its next report.
    pub verified: bool,
    /// Real but lower-than-requested flow: the command is confirmed without claiming the setpoint.
    pub partial: bool,
    /// A safety hold replaced this command while it waited; not a device fault, no retry needed.
    pub superseded: bool,
    /// Whether `applied_w` came from a sample younger than the freshness bound.
    pub flow_measured: bool,
    /// Device clock of the sample that supplied `applied_w`, when there was one.
    pub measured_at: Option<i64>,
}

impl WriteReceipt {
    /// A command the device echoed.
    pub const fn verified(applied_w: f64) -> Self {
        Self {
            applied_w,
            verified: true,
            partial: false,
            superseded: false,
            flow_measured: false,
            measured_at: None,
        }
    }

    /// Direction-correct flow that stayed below the command: measured, not a fault.
    pub const fn partial(applied_w: f64) -> Self {
        Self {
            applied_w,
            verified: false,
            partial: true,
            superseded: false,
            flow_measured: false,
            measured_at: None,
        }
    }

    /// A command the device accepted but has not echoed yet.
    pub const fn unverified() -> Self {
        Self {
            applied_w: 0.0,
            verified: false,
            partial: false,
            superseded: false,
            flow_measured: false,
            measured_at: None,
        }
    }

    /// A command a safety hold deliberately superseded: resolved without a fault.
    pub const fn superseded() -> Self {
        Self {
            applied_w: 0.0,
            verified: false,
            partial: false,
            superseded: true,
            flow_measured: false,
            measured_at: None,
        }
    }

    /// Attach the fresh sample that supplied the watts, making `applied_w` measurable downstream.
    #[must_use]
    pub const fn with_measurement(mut self, sample_at: i64) -> Self {
        self.flow_measured = true;
        self.measured_at = Some(sample_at);
        self
    }
}
