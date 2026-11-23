//! Backward dynamic program over the configured 0.05 kWh state lattice.
//!
//! Energy transitions are exact (`e + dt·p·η`); off-lattice energies are valued by linear
//! interpolation of the next layer's value function. Every candidate is ordered by the full
//! canonical six-key signature, so enumeration order never decides an equal-cost path.

use crate::optimizer::choose::best_action;
use crate::optimizer::context::ChooseContext;
use crate::optimizer::continuation::Continuation;
use crate::optimizer::curtailment;
use crate::optimizer::shared::SolverShared;
use crate::optimizer::{
    ActionGrid, BindingBound, OptimizerInput, OptimizerRefusal, OptimizerResult, Slot, StateLattice,
};

#[derive(Clone, Copy)]
pub(crate) struct ActionEval {
    pub(crate) power_w: i32,
    pub(crate) grid_eur: f64,
}

/// Solve the horizon, returning either an accepted path or a typed refusal.
pub fn solve(input: &OptimizerInput) -> Result<OptimizerResult, OptimizerRefusal> {
    crate::optimizer::validate::validate(input)?;
    let config = &input.config;
    let lattice = StateLattice::new(config)?;
    if !lattice.contains(config.start_kwh) {
        return Err(OptimizerRefusal::StartOutsideBounds {
            energy_kwh: config.start_kwh,
            floor_kwh: lattice.floor_kwh,
            ceiling_kwh: lattice.ceiling_kwh,
        });
    }
    let actions = ActionGrid::new(
        config.charge_power_w,
        config.discharge_power_w,
        config.power_steps,
    );
    let count = input.slots.len();
    let cells = lattice.points_kwh.len();
    let mut refused = Vec::new();
    let evals_by_slot: Vec<Vec<ActionEval>> = (0..count)
        .map(|index| evaluate_actions(input, &input.slots[index], index, &actions, &mut refused))
        .collect();
    let shared = SolverShared {
        cache: std::cell::RefCell::new(std::collections::HashMap::new()),
        bounds: refused,
    };
    let mut suffixes: Vec<Vec<Option<Continuation>>> = vec![vec![None; cells]; count];
    let cheapest_buy = input
        .slots
        .iter()
        .map(|slot| slot.buy_eur_per_kwh)
        .fold(f64::INFINITY, f64::min);
    let salvage = cheapest_buy / config.eta_charge;
    backwards(
        input,
        &lattice,
        &evals_by_slot,
        &mut suffixes,
        salvage,
        &shared,
    );
    crate::optimizer::finish::finish(input, &lattice, &suffixes, &evals_by_slot, salvage, &shared)
}

fn backwards(
    input: &OptimizerInput,
    lattice: &StateLattice,
    evals_by_slot: &[Vec<ActionEval>],
    suffixes: &mut [Vec<Option<Continuation>>],
    salvage: f64,
    shared: &SolverShared,
) {
    let mut refused = shared.bounds.clone();
    for index in (0..input.slots.len()).rev() {
        let context = ChooseContext {
            input,
            index,
            points: &lattice.points_kwh,
            evals_by_slot,
            suffixes,
            salvage,
            shared,
        };
        let chosen: Vec<Result<(f64, Continuation), OptimizerRefusal>> = lattice
            .points_kwh
            .iter()
            .map(|point| best_action(&context, *point, &mut refused))
            .collect();
        for (cell, result) in chosen.into_iter().enumerate() {
            if let Ok((_, suffix)) = result {
                suffixes[index][cell] = Some(suffix);
            }
        }
    }
}

/// Precompute every action's grid cost for one slot (independent of stored energy).
pub(crate) fn evaluate_actions(
    input: &OptimizerInput,
    slot: &Slot,
    index: usize,
    actions: &ActionGrid,
    refused: &mut Vec<BindingBound>,
) -> Vec<ActionEval> {
    let config = &input.config;
    let mut result = Vec::with_capacity(actions.powers_w.len());
    for &power_w in &actions.powers_w {
        let power_kw = f64::from(power_w) / 1000.0;
        match curtailment::solve(
            slot,
            index,
            power_kw,
            config.import_limit_kw,
            config.min_spread_eur_per_kwh,
        ) {
            Ok(solved) => result.push(ActionEval {
                power_w,
                grid_eur: solved.grid_cost_eur,
            }),
            Err(mut bounds) => refused.append(&mut bounds),
        }
    }
    result
}
