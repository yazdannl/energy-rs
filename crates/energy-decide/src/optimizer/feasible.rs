//! ONE action-feasibility predicate, shared by the DP, continuations, repair and final checks.

use crate::optimizer::charge_limit::max_charge_w_at_soc;
use crate::optimizer::dp::ActionEval;
use crate::optimizer::objective::next_energy_kwh;
use crate::optimizer::{OptimizerInput, OptimizerRefusal};

/// Exact energy after `power_w`, or `None` if the action is not executable at this state.
pub(crate) fn next_energy(
    input: &OptimizerInput,
    index: usize,
    energy: f64,
    power_w: i32,
    evals: &[ActionEval],
) -> Option<f64> {
    let config = &input.config;
    if !evals.iter().any(|action| action.power_w == power_w) {
        return None;
    }
    let power_kw = f64::from(power_w) / 1000.0;
    let soc = if config.ceiling_kwh > 0.0 {
        energy / config.ceiling_kwh
    } else {
        0.0
    };
    let charge_limit_kw = max_charge_w_at_soc(config.charge_power_w, soc) / 1000.0;
    if power_kw > charge_limit_kw + 1e-9 {
        return None;
    }
    let next = next_energy_kwh(
        energy,
        power_kw,
        input.slots[index].dt_h(),
        config.eta_charge,
        config.eta_discharge,
    );
    if next < config.hard_floor_kwh - 1e-9 || next > config.ceiling_kwh + 1e-9 {
        return None;
    }
    Some(next)
}

/// Independently validate every executed row against the same predicate after repair.
pub(crate) fn validate_schedule(
    input: &OptimizerInput,
    schedule: &[crate::optimizer::ScheduleSlot],
    evals_by_slot: &[Vec<ActionEval>],
) -> Result<(), OptimizerRefusal> {
    let mut energy = input.config.start_kwh;
    for (index, row) in schedule.iter().enumerate() {
        if row.slot != index || (row.energy_start_kwh - energy).abs() > 1e-9 {
            return Err(OptimizerRefusal::Infeasible {
                slot: index,
                binding_bounds: Vec::new(),
            });
        }
        let Some(next) = next_energy(input, index, energy, row.power_w, &evals_by_slot[index])
        else {
            return Err(OptimizerRefusal::Infeasible {
                slot: index,
                binding_bounds: Vec::new(),
            });
        };
        if (next - row.energy_end_kwh).abs() > 1e-9 {
            return Err(OptimizerRefusal::Infeasible {
                slot: index,
                binding_bounds: Vec::new(),
            });
        }
        let config = &input.config;
        let Ok(solved) = crate::optimizer::solve_curtailment(
            &input.slots[index],
            index,
            f64::from(row.power_w) / 1000.0,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        ) else {
            return Err(OptimizerRefusal::Infeasible {
                slot: index,
                binding_bounds: Vec::new(),
            });
        };
        if (solved.curtail_kw - row.curtail_kw).abs() > 1e-9
            || (solved.grid_kw - row.grid_kw).abs() > 1e-9
            || (solved.grid_cost_eur - row.grid_eur).abs() > 1e-9
        {
            return Err(OptimizerRefusal::Infeasible {
                slot: index,
                binding_bounds: Vec::new(),
            });
        }
        energy = next;
    }
    Ok(())
}

/// Bounds explaining why an action is not executable, for typed refusals.
pub(crate) fn rejection_bounds(
    input: &OptimizerInput,
    index: usize,
    power_w: i32,
) -> Vec<crate::optimizer::BindingBound> {
    let config = &input.config;
    crate::optimizer::solve_curtailment(
        &input.slots[index],
        index,
        f64::from(power_w) / 1000.0,
        config.import_limit_kw,
        config.min_spread_eur_per_kwh,
    )
    .err()
    .unwrap_or_default()
}

/// Add a bound once, keeping refusals small and ordered.
pub(crate) fn push_bound(
    bounds: &mut Vec<crate::optimizer::BindingBound>,
    bound: crate::optimizer::BindingBound,
) {
    if !bounds.iter().any(|existing| existing == &bound) {
        bounds.push(bound);
    }
}
