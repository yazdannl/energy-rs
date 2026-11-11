//! The optimizer configuration derived from the plan input.

use energy_opt::Config;

use crate::plan::aggregate::Aggregate;
use crate::plan::{PlanInput, SLOT_MINUTES};

/// The shipped solver policy for a quarter-hour plan.
pub(crate) fn solver_config(input: &PlanInput, aggregate: &Aggregate, salvage: f64) -> Config {
    let mut config = Config::new(aggregate.spec, aggregate.start_kwh);
    config.slot_minutes = SLOT_MINUTES;
    config.soc_step_kwh = 0.05;
    config.power_steps = 24;
    config.max_states = 400;
    config.allow_grid_charge = true;
    // Export is allowed unless the operator banned it. There is no static price floor: whether a
    // sale pays is a horizon question the DP answers itself (sell the peak, buy the valley), and a
    // "never below stored cost" rule forbids sales that are demonstrably profitable once the
    // replacement is cheaper. `zero_export` is the only ban, enforced here and by the ladder.
    config.allow_export = !input.policy.zero_export;
    config.refuse_battery_export = input.policy.zero_export;
    // Energy left at the horizon is worth what it would cost to put back at the cheapest known
    // slot, grossed up for the charge efficiency: without a salvage value the DP happily sells
    // energy it never replaces, which is the one thing this policy must not do.
    config.salvage = salvage.max(0.0);
    config.wear_eur_per_kwh = input.wear_eur_per_kwh;
    config.target_penalty = input.policy.peak_reserve_value_eur_per_kwh;
    config.sell_floor_eur_per_kwh = None;
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
