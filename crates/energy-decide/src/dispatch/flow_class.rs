//! Ordered, mutually exclusive receipt predicates from the M4 outcome table.

/// One of the thirteen ordered flow predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowClass {
    /// Invalid status or status contradicting measured sign.
    StatusInconsistent,
    /// Idle status with more than 30 W flowing.
    IdleWithFlow,
    /// Unknown status with more than 30 W flowing.
    UnknownDirectionFlow,
    /// Hold with flow and a valid directional status.
    HoldUnexpectedFlow,
    /// Valid status opposite the command with flow.
    WrongDirection,
    /// Unknown status with no material flow.
    UnknownDirectionNoFlow,
    /// Opposite valid status with no material flow.
    OppositeDirectionNoFlow,
    /// Hold with a directional status and no material flow.
    HoldStatusDirectional,
    /// Hold with idle status and no material flow.
    HoldVerified,
    /// Nonzero command with no material flow.
    NoFlow,
    /// Direction matches and error is within tolerance.
    FullFlow,
    /// Direction matches but flow is materially below tolerance.
    PartialFlow,
    /// Direction matches but flow exceeds tolerance.
    OverFlow,
}

impl FlowClass {
    /// Whether this predicate is an anomaly under the M4 outcome table.
    pub fn is_anomaly(self) -> bool {
        !matches!(
            self,
            Self::HoldVerified | Self::FullFlow | Self::PartialFlow
        )
    }

    /// Whether this predicate alone can confirm a command.
    pub fn confirms(self) -> bool {
        matches!(self, Self::HoldVerified | Self::FullFlow)
    }
}
