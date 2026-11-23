//! Safety action supplied by the M5 consumer port.

/// Action a safety gate requires instead of the economic command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SafetyAction {
    /// Explicit neutral hold.
    NeutralHold,
    /// Emergency charge at the target chosen by M5.
    EmergencyCharge { target_w: f64 },
}

impl SafetyAction {
    /// Power M4 must write for this action.
    pub fn target_w(self) -> f64 {
        match self {
            Self::NeutralHold => 0.0,
            Self::EmergencyCharge { target_w } => target_w,
        }
    }
}
