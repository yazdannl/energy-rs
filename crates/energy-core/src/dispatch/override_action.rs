//! What an operator override asks one pack to do.

/// One pack's manual command, before it meets the safety ladder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverrideAction {
    /// Charge at this many watts (still bounded by SoC, latch, ramp and grid).
    Charge {
        /// Requested charge power, W.
        watts: f64,
    },
    /// Stay neutral, whatever the plan says.
    Hold,
    /// Get out of the way for this pack: hold it and exclude it from the plan's split.
    Off,
}

impl OverrideAction {
    /// The setpoint this action asks for, W (positive charging).
    pub const fn watts(self) -> f64 {
        match self {
            Self::Charge { watts } => watts,
            Self::Hold | Self::Off => 0.0,
        }
    }

    /// A stable slug for logs and the dispatch reason.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Charge { .. } => "override_charge",
            Self::Hold => "override_hold",
            Self::Off => "override_off",
        }
    }
}
