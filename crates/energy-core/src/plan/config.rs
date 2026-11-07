//! The optimizer configuration derived from the plan input.

use energy_opt::Config;

use crate::plan::aggregate::Aggregate;
use crate::plan::{PlanInput, SLOT_MINUTES};

/// The shipped solver policy for a quarter-hour plan.
pub(crate) fn solver_config(input: &PlanInput, aggregate: &Aggregate, floor: f64) -> Config {
    let mut config = Config::new(aggregate.spec, aggregate.start_kwh);
    config.slot_minutes = SLOT_MINUTES;
    config.soc_step_kwh = 0.05;
    config.power_steps = 24;
    config.max_states = 400;
    config.allow_grid_charge = true;
    // Survey §7 trap 12: the optimizer ships with export disabled until the settlement says
    // otherwise; the plan still records the feed-in so safety and reporting can judge a discharge.
    config.allow_export = false;
    config.wear_eur_per_kwh = input.wear_eur_per_kwh;
    config.target_penalty = input.target_penalty_eur_per_kwh;
    config.sell_floor_eur_per_kwh = Some(floor);
    config.e_target = terminal_target(input, aggregate);
    config
}

fn terminal_target(input: &PlanInput, aggregate: &Aggregate) -> Option<f64> {
    input.terminal_target_pct.map(|pct| {
        aggregate.spec.e_min_kwh
            + (aggregate.spec.e_max_kwh - aggregate.spec.e_min_kwh) * pct / 100.0
    })
}
