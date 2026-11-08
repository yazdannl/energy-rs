//! The optimizer configuration derived from the plan input.

use energy_opt::Config;

use crate::plan::aggregate::Aggregate;
use crate::plan::{PlanInput, SLOT_MINUTES};

/// The shipped solver policy for a quarter-hour plan.
pub(crate) fn solver_config(
    input: &PlanInput,
    aggregate: &Aggregate,
    floor: f64,
    break_even: Option<f64>,
) -> Config {
    let mut config = Config::new(aggregate.spec, aggregate.start_kwh);
    config.slot_minutes = SLOT_MINUTES;
    config.soc_step_kwh = 0.05;
    config.power_steps = 24;
    config.max_states = 400;
    config.allow_grid_charge = true;
    // The DP's own sell floor is unused: its second clause (`base > 0 && buy < floor`) would block
    // self-consumption below the break-even, which the policy forbids. The gate is a post-solve
    // clamp instead (`plan::clamp`), and the DP may export whenever a break-even is armed.
    config.allow_export = break_even.is_some();
    config.wear_eur_per_kwh = input.wear_eur_per_kwh;
    config.target_penalty = input.policy.peak_reserve_value_eur_per_kwh;
    config.sell_floor_eur_per_kwh = break_even.is_none().then_some(floor);
    config.e_target = terminal_target(input, aggregate);
    config
}

/// The soft end-of-horizon target: the operator's explicit target, else the peak guideline.
fn terminal_target(input: &PlanInput, aggregate: &Aggregate) -> Option<f64> {
    if let Some(pct) = input.terminal_target_pct {
        return Some(
            aggregate.spec.e_min_kwh
                + (aggregate.spec.e_max_kwh - aggregate.spec.e_min_kwh) * pct / 100.0,
        );
    }
    input
        .policy
        .peak_target_kwh(aggregate.capacity_kwh, aggregate.start_kwh)
}
