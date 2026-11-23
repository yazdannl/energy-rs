//! Canonical polish: whole-path replays use the same feasibility and tie keys as the DP.

use crate::optimizer::dp::ActionEval;
use crate::optimizer::feasible;
use crate::optimizer::finish::breakdown;
use crate::optimizer::key::path_key;
use crate::optimizer::objective::wear_eur;
use crate::optimizer::{ObjectiveBreakdown, OptimizerInput, ScheduleSlot, path, solve_curtailment};

fn replay(
    input: &OptimizerInput,
    powers: &[i32],
    evals_by_slot: &[Vec<ActionEval>],
    salvage: f64,
) -> Option<(Vec<ScheduleSlot>, ObjectiveBreakdown)> {
    let config = &input.config;
    let mut energy = config.start_kwh;
    let mut schedule = Vec::with_capacity(powers.len());
    for (index, &power_w) in powers.iter().enumerate() {
        let next = feasible::next_energy(input, index, energy, power_w, &evals_by_slot[index])?;
        let solved = solve_curtailment(
            &input.slots[index],
            index,
            f64::from(power_w) / 1000.0,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        )
        .ok()?;
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
    let objective = breakdown(
        input,
        &schedule,
        energy,
        config.effective_target_kwh(),
        salvage,
    );
    Some((schedule, objective))
}

fn key_of(
    schedule: &[ScheduleSlot],
    objective: &ObjectiveBreakdown,
) -> crate::optimizer::key::CanonicalKey {
    let powers: Vec<i32> = schedule.iter().map(|row| row.power_w).collect();
    let energies: Vec<f64> = schedule.iter().map(|row| row.energy_end_kwh).collect();
    let (first, squared, nonzero, energy_sig, power_sig) = path::fields(0, &powers, &energies);
    path_key(
        objective.total_eur,
        first,
        squared,
        nonzero,
        &energy_sig,
        &power_sig,
    )
}

/// Improve the canonical key with exact whole-path replays; at most three sweeps.
pub(crate) fn canonical_repair(
    input: &OptimizerInput,
    schedule: &mut Vec<ScheduleSlot>,
    evals_by_slot: &[Vec<ActionEval>],
    salvage: f64,
) {
    for _ in 0..3 {
        let mut improved = false;
        let mut powers: Vec<i32> = schedule.iter().map(|row| row.power_w).collect();
        for index in 0..powers.len() {
            for &power in &evals_by_slot[index]
                .iter()
                .map(|a| a.power_w)
                .collect::<Vec<_>>()
            {
                if power == powers[index] {
                    continue;
                }
                let mut trial_powers = powers.clone();
                trial_powers[index] = power;
                let Some((trial, trial_objective)) =
                    replay(input, &trial_powers, evals_by_slot, salvage)
                else {
                    continue;
                };
                let current_objective = breakdown(
                    input,
                    schedule,
                    schedule.last().map_or(0.0, |row| row.energy_end_kwh),
                    input.config.effective_target_kwh(),
                    salvage,
                );
                if key_of(&trial, &trial_objective) < key_of(schedule, &current_objective) {
                    *schedule = trial;
                    powers = trial_powers;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
}
