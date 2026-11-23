//! Status-pair mapping from the M4 outcome table.

/// Direction implied by a pack's (AC, DC) status pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowStatus {
    /// Charge: (2, 1).
    Charge,
    /// Discharge: (1, 2).
    Discharge,
    /// Idle: (0, 0).
    Idle,
    /// A non-negative but unrecognised pair.
    Unknown,
    /// An impossible negative code.
    Invalid,
}

/// Map a reported status pair to its direction.
pub fn status_direction(ac_status: i32, dc_status: i32) -> FlowStatus {
    if ac_status < 0 || dc_status < 0 {
        return FlowStatus::Invalid;
    }
    match (ac_status, dc_status) {
        (2, 1) => FlowStatus::Charge,
        (1, 2) => FlowStatus::Discharge,
        (0, 0) => FlowStatus::Idle,
        _ => FlowStatus::Unknown,
    }
}
