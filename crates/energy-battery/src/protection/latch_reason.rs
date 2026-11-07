//! Why a cell-voltage latch is where it is.

use serde::{Deserialize, Serialize};

/// The reason a latch was last touched.
///
/// The reason and its timestamp survive a restart with the latch, because "still blocked because the
/// reading is stale" and "blocked because a cell was at 2.94 V" are different states to an operator,
/// even though the dispatcher treats them identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatchReason {
    /// No reading exists for this pack yet; anything that was latched stays latched.
    MissingReading,
    /// The last reading is older than the stale limit; the previous latch is kept.
    StaleReading,
    /// A cell is below the cutoff: discharge blocked and an emergency charge requested.
    BelowCutoff,
    /// A cell is between cutoff and resume: discharge blocked, no forced charge.
    BelowResume,
    /// A cell is at or above resume: the latch is clear.
    AboveResume,
}

impl LatchReason {
    /// A stable slug for logs and the UI.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::MissingReading => "cell_missing_reading",
            Self::StaleReading => "cell_stale_reading",
            Self::BelowCutoff => "cell_below_cutoff",
            Self::BelowResume => "cell_below_resume",
            Self::AboveResume => "cell_above_resume",
        }
    }
}
