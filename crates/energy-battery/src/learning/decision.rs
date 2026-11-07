//! What the learner did with its inputs, and why.

use serde::{Deserialize, Serialize};

/// The outcome of one learning pass.
///
/// `applied` answers the only question the planner asks ("did a measurement publish a new number?"),
/// while the variant says why. `KeptPrevious` is the important one: thin data must not wipe a
/// confident profile, so the previous value stands and the pass is reported as *not* applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningDecision {
    /// Published: enough usable measurements, and the estimate sits inside its band.
    Applied,
    /// Published at the band's upper edge: the raw estimate is above anything the pack may hold.
    ClampedHigh,
    /// Published at the band's lower edge: the raw estimate is below it.
    ClampedLow,
    /// Too few usable sessions this pass; nothing new is published.
    LowConfidence,
    /// No usable session at all.
    NoSessions,
    /// The previous confident estimate stands; this pass had too little data to replace it.
    KeptPrevious,
}

impl LearningDecision {
    /// Whether this decision publishes a value the planner may use.
    pub const fn applied(self) -> bool {
        matches!(self, Self::Applied | Self::ClampedHigh | Self::ClampedLow)
    }

    /// Whether the value in hand is a measurement (new, clamped, or carried over).
    pub const fn publishable(self) -> bool {
        self.applied() || matches!(self, Self::KeptPrevious)
    }

    /// A stable slug for logs and the UI (`capacity_learned*`).
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Applied => "capacity_learned",
            Self::ClampedHigh => "capacity_learned_clamped_high",
            Self::ClampedLow => "capacity_learned_clamped_low",
            Self::LowConfidence => "capacity_learned_low_confidence",
            Self::NoSessions => "capacity_learned_no_sessions",
            Self::KeptPrevious => "capacity_learned_kept_previous",
        }
    }
}
