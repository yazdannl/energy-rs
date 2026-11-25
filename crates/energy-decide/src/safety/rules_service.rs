//! Unconditional physical service-limit rule.

use crate::dispatch::SafetyAction;
use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::safety::rules::{
    Candidate, blocked, candidate, coordinated_w, fleet_baseline_w, service_projection,
};
use crate::safety::rules_limits::physical_limits;
use crate::safety::{Rule, SafetyInput};

fn reduction(input: &SafetyInput, baseline: f64) -> SafetyAction {
    let headroom_w = input.physical_service_limit_w - baseline - coordinated_w(input);
    if headroom_w < 0.0 {
        SafetyAction::ProtectiveDischarge {
            target_w: headroom_w,
        }
    } else {
        SafetyAction::NeutralHold
    }
}

pub(crate) fn physical_service(input: &SafetyInput) -> Option<Candidate> {
    if input.plan_w.abs() <= HOLD_DEADBAND_W {
        return None;
    }
    let Some(projected) = service_projection(input, input.plan_w) else {
        let item = blocked(
            Rule::PhysicalServiceLimit,
            "service_headroom_unavailable",
            vec!["grid_sample_missing_or_stale"],
        );
        return Some(item);
    };
    if projected <= input.physical_service_limit_w {
        return None;
    }
    let baseline = fleet_baseline_w(input).unwrap_or_default();
    let action = reduction(input, baseline);
    let required_w = action.target_w();
    let limits = physical_limits(input, false, required_w);
    if limits.is_empty() {
        let result = candidate(Rule::PhysicalServiceLimit, Some(action));
        return Some(result);
    }
    let result = blocked(Rule::PhysicalServiceLimit, "physical_service_limit", limits);
    Some(result)
}
