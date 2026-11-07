//! One pack's share of a fleet setpoint.

/// The watts one pack is asked to take (positive charging) or deliver (negative discharging).
#[derive(Debug, Clone, PartialEq)]
pub struct PackAllocation {
    /// Stable pack id.
    pub pack_id: String,
    /// Commanded AC power, W.
    pub watts: f64,
}
