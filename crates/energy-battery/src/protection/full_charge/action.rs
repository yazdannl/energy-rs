//! What the maintenance scheduler wants the dispatcher to do.

use chrono::{DateTime, Utc};

/// One evaluation's answer for the full-charge maintenance.
///
/// The action carries the already clamped charge limit, so the dispatcher does not have to know the
/// maintenance settings to obey it. `Release` is returned exactly once at the end of a window; the
/// caller drops its maintenance override and subsequent evaluations answer `Idle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FullChargeAction {
    /// Not due, disabled, or just released: no maintenance override.
    Idle,
    /// Charge towards the target at `limit_w`.
    Charge {
        /// Requested charge power, already clamped, W.
        limit_w: u32,
        /// SoC to stop at.
        target_soc_pct: f64,
    },
    /// The target is reached: keep the pack there until `until` so the BMS can balance.
    Hold {
        /// Requested hold power, W.
        limit_w: u32,
        /// End of the hold window.
        until: DateTime<Utc>,
    },
    /// The window is over (or a run expired): drop the override now.
    Release,
}

impl FullChargeAction {
    /// Whether a maintenance override is in force this evaluation.
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Charge { .. } | Self::Hold { .. } | Self::Release
        )
    }

    /// The charge power the action asks for, if any.
    pub const fn limit_w(self) -> Option<u32> {
        match self {
            Self::Charge { limit_w, .. } | Self::Hold { limit_w, .. } => Some(limit_w),
            Self::Idle | Self::Release => None,
        }
    }
}
