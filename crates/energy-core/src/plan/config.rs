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
    // A 24-step lattice cannot charge a 0.10 kW PV surplus without importing the difference, nor
    // serve a 0.88 kW load without spilling 0.13 kW to the grid. The two grid-neutral powers are
    // what makes the plan capture the sun and stop selling battery energy below break-even.
    config.align_actions = true;
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

/// The terminal target handed to the solver: a soft guideline raised to the hard floor.
///
/// The peak reserve is a soft end-of-horizon guideline and `reserve_soc_pct` is a hard floor the
/// clamp ladder will not let the fleet cross. A guideline below that floor is a configuration
/// contradiction: the solver would be penalised for ending above the target while the ladder
/// forbids ending below it, so every solve is reported `terminal_target_unreachable` even though
/// the SoC path correctly stops at the floor. There is no legitimate case for keeping such a
/// target -- deliberately setting `peak_reserve_pct` below `reserve_soc_pct` still asks to spend
/// energy the dispatcher refuses to deliver -- so the effective target is the greater of the two.
/// The soft guideline itself is reported separately in `peak_reserve_kwh`; only this effective
/// target is stored in `terminal_target_kwh` and used for the feasibility check.
fn terminal_target(input: &PlanInput, aggregate: &Aggregate) -> Option<f64> {
    let soft = if let Some(pct) = input.terminal_target_pct {
        Some(
            aggregate.spec.e_min_kwh
                + (aggregate.spec.e_max_kwh - aggregate.spec.e_min_kwh) * pct / 100.0,
        )
    } else {
        input
            .policy
            .peak_target_kwh(aggregate.capacity_kwh, aggregate.start_kwh)
    }?;
    // The upper bound keeps an out-of-band start or an oversized guideline inside the physical
    // band; the floor is the part that fixes the contradiction described above.
    Some(
        soft.max(aggregate.spec.e_min_kwh)
            .min(aggregate.spec.e_max_kwh),
    )
}
