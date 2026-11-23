//! Replay the exact path, validate it after repair and split every objective term.

use crate::optimizer::choose::best_action;
use crate::optimizer::context::ChooseContext;
use crate::optimizer::dp::ActionEval;
use crate::optimizer::feasible;
use crate::optimizer::key::NO_DISCHARGE;
use crate::optimizer::objective::{round_eur, wear_eur};
use crate::optimizer::shared::SolverShared;
use crate::optimizer::{ObjectiveBreakdown, path, repair, solve_curtailment};
use crate::optimizer::{OptimizerInput, OptimizerRefusal, OptimizerResult, ScheduleSlot, TieBreak};

/// Replay the canonical choices forward from `e_0`, repair, validate, then report.
pub(crate) fn finish(
    input: &OptimizerInput,
    lattice: &crate::optimizer::StateLattice,
    suffixes: &[Vec<Option<crate::optimizer::continuation::Continuation>>],
    evals_by_slot: &[Vec<ActionEval>],
    salvage: f64,
    shared: &SolverShared,
) -> Result<OptimizerResult, OptimizerRefusal> {
    let mut schedule = reconstruct(input, lattice, suffixes, evals_by_slot, salvage, shared)?;
    repair::canonical_repair(input, &mut schedule, evals_by_slot, salvage);
    feasible::validate_schedule(input, &schedule, evals_by_slot)?;
    let target = input.config.effective_target_kwh();
    let energy = schedule
        .last()
        .map_or(input.config.start_kwh, |row| row.energy_end_kwh);
    let objective = breakdown(input, &schedule, energy, target, salvage);
    let tie = tie_break(&schedule, &objective);
    Ok(OptimizerResult {
        schedule,
        objective,
        terminal_kwh: energy,
        effective_target_kwh: target,
        salvage_eur_per_stored_kwh: salvage,
        tie_break: tie,
        binding_bounds: Vec::new(),
    })
}

fn reconstruct(
    input: &OptimizerInput,
    lattice: &crate::optimizer::StateLattice,
    suffixes: &[Vec<Option<crate::optimizer::continuation::Continuation>>],
    evals_by_slot: &[Vec<ActionEval>],
    salvage: f64,
    shared: &SolverShared,
) -> Result<Vec<ScheduleSlot>, OptimizerRefusal> {
    let config = &input.config;
    let mut refused = shared.bounds.clone();
    let mut energy = config.start_kwh;
    let mut schedule = Vec::with_capacity(input.slots.len());
    for index in 0..input.slots.len() {
        let context = ChooseContext {
            input,
            index,
            points: &lattice.points_kwh,
            evals_by_slot,
            suffixes,
            salvage,
            shared,
        };
        let (_, chosen) = best_action(&context, energy, &mut refused)?;
        let power_w = chosen.powers[0];
        let next = feasible::next_energy(input, index, energy, power_w, &evals_by_slot[index])
            .ok_or(OptimizerRefusal::Infeasible {
                slot: index,
                binding_bounds: refused.clone(),
            })?;
        let solved = solve_curtailment(
            &input.slots[index],
            index,
            f64::from(power_w) / 1000.0,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        )
        .map_err(|bounds| OptimizerRefusal::Infeasible {
            slot: index,
            binding_bounds: bounds,
        })?;
        schedule.push(ScheduleSlot {
            slot: index,
            t_start: input.slots[index].t_start,
            power_w,
            energy_start_kwh: energy,
            energy_end_kwh: next,
            curtail_kw: solved.curtail_kw,
            grid_kw: solved.grid_kw,
            grid_eur: solved.grid_cost_eur,
            wear_eur: wear_eur(energy, next, config.wear_eur_per_stored_kwh),
            policy_preference_eur: solved.policy_preference_eur,
        });
        energy = next;
    }
    Ok(schedule)
}

fn tie_break(schedule: &[ScheduleSlot], objective: &ObjectiveBreakdown) -> TieBreak {
    let no_discharge = schedule.len();
    let powers: Vec<i32> = schedule.iter().map(|row| row.power_w).collect();
    let energies: Vec<f64> = schedule.iter().map(|row| row.energy_end_kwh).collect();
    let (first, squared, nonzero, energy_sig, power_sig) = path::fields(0, &powers, &energies);
    TieBreak {
        objective_rounded_eur: round_eur(objective.total_eur),
        first_discharge_slot_index: if first == NO_DISCHARGE {
            no_discharge
        } else {
            first
        },
        sum_power_squared: squared as f64,
        nonzero_slot_count: nonzero,
        energy_signature_wh: energy_sig,
        power_signature_w: power_sig,
        tie_resolved_by: "canonical_signatures",
    }
}

pub(crate) fn breakdown(
    input: &OptimizerInput,
    schedule: &[ScheduleSlot],
    terminal_kwh: f64,
    target: f64,
    salvage: f64,
) -> ObjectiveBreakdown {
    let config = &input.config;
    let mut grid = 0.0;
    let mut wear = 0.0;
    let mut preference = 0.0;
    let mut policy_spread = 0.0;
    for row in schedule {
        grid += row.grid_eur;
        wear += row.wear_eur;
        preference += row.policy_preference_eur;
        if let Ok(solved) = solve_curtailment(
            &input.slots[row.slot],
            row.slot,
            f64::from(row.power_w) / 1000.0,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        ) {
            policy_spread += solved.policy_min_spread_eur;
        }
    }
    let salvage_eur = -salvage * terminal_kwh;
    let penalty = config.target_penalty_eur_per_kwh
        * (target - terminal_kwh - config.terminal_deadband_kwh).max(0.0);
    ObjectiveBreakdown {
        grid_eur: grid,
        wear_eur: wear,
        policy_min_spread_eur: policy_spread,
        salvage_eur,
        terminal_penalty_eur: penalty,
        total_eur: grid + wear + salvage_eur + penalty,
        policy_preference_eur: preference,
    }
}
