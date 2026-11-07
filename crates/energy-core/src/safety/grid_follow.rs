//! The grid-follow correction: a bounded move towards the measured grid flow.

use crate::safety::{GRID_FOLLOW_BUDGET_RATIO, GRID_FOLLOW_DEADBAND_W, GRID_FOLLOW_MIN_BUDGET_W};

/// Grid-follow policy (survey §7 trap 11).
///
/// The correction is bounded by the batteries' own capability (`max(500 W, 0.5 × capability)`),
/// never by a fraction of the plan power: a planned hold is exactly when a deviation needs
/// correcting, so a fraction of zero would freeze the loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridFollow {
    /// Ignore deviations at or below this many watts.
    pub deadband_w: f64,
    /// Minimum correction budget, W.
    pub min_budget_w: f64,
    /// Budget as a share of the fleet's capability.
    pub budget_ratio: f64,
}

impl Default for GridFollow {
    fn default() -> Self {
        Self {
            deadband_w: GRID_FOLLOW_DEADBAND_W,
            min_budget_w: GRID_FOLLOW_MIN_BUDGET_W,
            budget_ratio: GRID_FOLLOW_BUDGET_RATIO,
        }
    }
}

impl GridFollow {
    /// The corrected target, or `target_w` unchanged inside the deadband.
    pub fn correct(
        &self,
        target_w: f64,
        measured_grid_w: f64,
        planned_grid_kw: f64,
        capability_w: f64,
    ) -> f64 {
        let delta = -(measured_grid_w / 1000.0 - planned_grid_kw) * 1000.0;
        if !delta.is_finite() || delta.abs() <= self.deadband_w {
            return target_w;
        }
        let budget = (capability_w * self.budget_ratio).max(self.min_budget_w);
        target_w + delta.clamp(-budget, budget)
    }
}
