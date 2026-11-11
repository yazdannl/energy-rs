//! The grid-follow correction: a bounded move towards the measured grid flow.

use crate::safety::{GRID_FOLLOW_DEADBAND_W, GRID_FOLLOW_MIN_BUDGET_W};

/// Grid-follow policy (survey §7 trap 11).
///
/// The correction is bounded by the batteries' own capability, never by a fraction of the plan
/// power (a planned hold is exactly when a deviation needs correcting) and never by a fraction of
/// the capability either: the correction is a setpoint recomputed from the plan target every tick,
/// so any budget below the capability would cap the same move on every tick and leave the residual
/// standing for good. Rate is bounded by the ladder's per-pack ramp, not by this number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridFollow {
    /// Ignore deviations at or below this many watts.
    pub deadband_w: f64,
    /// Minimum correction budget, W (a fleet smaller than this still corrects this much).
    pub min_budget_w: f64,
}

impl Default for GridFollow {
    fn default() -> Self {
        Self {
            deadband_w: GRID_FOLLOW_DEADBAND_W,
            min_budget_w: GRID_FOLLOW_MIN_BUDGET_W,
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
        let budget = capability_w.max(self.min_budget_w);
        target_w + delta.clamp(-budget, budget)
    }
}
