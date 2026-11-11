//! The grid-follow correction: one bounded move towards the desired grid flow.

use crate::safety::{FollowRequest, GRID_FOLLOW_DEADBAND_W, GRID_FOLLOW_MIN_BUDGET_W};

/// Grid-follow policy (survey §7 trap 11).
///
/// The correction is **incremental**: it adds the meter's error to the battery's own measured
/// output, not to the plan's setpoint. Adding it to the plan re-applies a correction the meter
/// already reflects, which alternates between over- and under-shooting on every tick instead of
/// settling. With the measured output as the base, commanding `battery + error` makes the resulting
/// grid exactly the desired one, whatever the house and the sun are doing.
///
/// Inside the deadband the battery stays where it is: there is nothing to correct, and returning the
/// plan's setpoint instead would stop the very battery whose output is holding the meter where it
/// is — the loop would then correct again on the next tick, alternating for ever.
///
/// The plan's setpoint is what the caller uses when there is no follow at all (a stale tick, no
/// slot, no meter) or when the slot is deliberately exchanging energy.
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
    /// The corrected target: the plan's setpoint inside the deadband, one bounded incremental move
    /// outside it.
    pub fn correct(&self, request: &FollowRequest) -> f64 {
        // The error is a grid move; the same number moves the battery the other way (a fleet that
        // charges one more watt imports one more watt).
        let base = if request.battery_w.is_finite() {
            request.battery_w
        } else {
            request.plan_w
        };
        let error = request.desired_grid_w - request.measured_grid_w;
        if !error.is_finite() || error.abs() <= self.deadband_w {
            return base;
        }
        let budget = request.capability_w.max(self.min_budget_w);
        (base + error).clamp(-budget, budget)
    }
}
