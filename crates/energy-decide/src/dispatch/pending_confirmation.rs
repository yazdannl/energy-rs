//! Deferred confirmation state; no confirmation wait may enter the tick.

/// Pending command evidence carried across ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingConfirmation {
    /// Commanded power in W.
    pub command_w: f64,
    /// Issue instant, epoch milliseconds.
    pub issued_at_ms: i64,
    /// Echo instant, when the echo arrived with the ack.
    pub echo_at_ms: Option<i64>,
    /// Latest instant at which flow must have reached the command.
    pub ramp_deadline_ms: i64,
    /// First in-band measurement: the ramp ends here.
    pub first_in_band_ms: Option<i64>,
    /// In-band reports after the first one.
    pub samples_after_band: usize,
    /// No-flow reports after the ramp deadline.
    pub post_ramp_no_flow: usize,
    /// Last ordered report sequence accepted.
    pub message_id: u32,
}

impl PendingConfirmation {
    /// Start pending confirmation for one command.
    pub fn new(command_w: f64, issued_at_ms: i64, echo: Option<i64>, ramp_max_ms: u64) -> Self {
        let allowance = i64::try_from(ramp_max_ms).unwrap_or(i64::MAX);
        Self {
            command_w,
            issued_at_ms,
            echo_at_ms: echo,
            ramp_deadline_ms: issued_at_ms.saturating_add(allowance),
            first_in_band_ms: None,
            samples_after_band: 0,
            post_ramp_no_flow: 0,
            message_id: 0,
        }
    }

    /// Whether a fresh in-band report has ended the ramp.
    pub fn ramp_ended(&self) -> bool {
        self.first_in_band_ms.is_some()
    }

    /// Whether two in-band samples have confirmed the command.
    pub fn in_band_confirmed(&self) -> bool {
        self.samples_after_band >= 1
    }
}
