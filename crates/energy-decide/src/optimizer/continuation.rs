//! Off-lattice continuations are replayed, never interpolated into an impossible path.

use crate::optimizer::choose::INFEASIBLE;
use crate::optimizer::context::ChooseContext;
use crate::optimizer::key::{CanonicalKey, NO_DISCHARGE, path_key};
use crate::optimizer::objective::terminal_eur;
use crate::optimizer::path;

/// A concrete, already replayed suffix of actions starting after the current slot.
#[derive(Clone, Debug)]
pub(crate) struct Continuation {
    pub(crate) cost_eur: f64,
    pub(crate) first_discharge: usize,
    pub(crate) power_squared_w2: i64,
    pub(crate) nonzero_slots: usize,
    pub(crate) energy_signature: Vec<i32>,
    pub(crate) power_signature: Vec<i32>,
    pub(crate) powers: Vec<i32>,
}

impl Continuation {
    fn terminal(cost_eur: f64) -> Self {
        Self {
            cost_eur,
            first_discharge: NO_DISCHARGE,
            power_squared_w2: 0,
            nonzero_slots: 0,
            energy_signature: Vec::new(),
            power_signature: Vec::new(),
            powers: Vec::new(),
        }
    }
}

fn terminal(context: &ChooseContext<'_>, energy: f64) -> Continuation {
    let config = &context.input.config;
    Continuation::terminal(terminal_eur(
        energy,
        config.effective_target_kwh(),
        config.terminal_deadband_kwh,
        config.target_penalty_eur_per_kwh,
        context.salvage,
    ))
}

fn replayed(
    context: &ChooseContext<'_>,
    layer: usize,
    energy: f64,
    powers: &[i32],
) -> Option<(Continuation, CanonicalKey)> {
    let (cost, energies) = path::cost(
        context.input,
        energy,
        layer,
        powers,
        context.evals_by_slot,
        context.salvage,
    )?;
    let (first, squared, nonzero, energy_sig, power_sig) = path::fields(layer, powers, &energies);
    let key = path_key(cost, first, squared, nonzero, &energy_sig, &power_sig);
    Some((
        Continuation {
            cost_eur: cost,
            first_discharge: first,
            power_squared_w2: squared,
            nonzero_slots: nonzero,
            energy_signature: energy_sig,
            power_signature: power_sig,
            powers: powers.to_vec(),
        },
        key,
    ))
}

/// Pick the cheapest continuation executable from the exact energy. Candidate action lists come
/// from nearby lattice nodes; each is replayed through the shared feasibility predicate, so a
/// path that would cross the floor or ceiling from this exact state is discarded.
pub(crate) fn best(context: &ChooseContext<'_>, energy: f64) -> Continuation {
    let key = (context.index + 1, energy.to_bits());
    if let Some(cached) = context.shared.cache.borrow().get(&key) {
        return cached.clone();
    }
    let layer = context.index + 1;
    if layer == context.input.slots.len() {
        return terminal(context, energy);
    }
    let lower = context
        .points
        .partition_point(|point| *point <= energy)
        .saturating_sub(1);
    let mut best: Option<(Continuation, CanonicalKey)> = None;
    for radius in 0..=2 {
        let from = lower.saturating_sub(radius);
        let to = (lower + radius + 1).min(context.points.len());
        for cell in from..to {
            let Some(suffix) = &context.suffixes[layer][cell] else {
                continue;
            };
            if let Some(candidate) = replayed(context, layer, energy, &suffix.powers)
                && best
                    .as_ref()
                    .is_none_or(|(_, incumbent)| candidate.1 < *incumbent)
            {
                best = Some(candidate);
            }
        }
        if best.is_some() && radius > 0 {
            break;
        }
    }
    let result = best.map_or_else(
        || Continuation::terminal(INFEASIBLE),
        |(continuation, _)| continuation,
    );
    context
        .shared
        .cache
        .borrow_mut()
        .insert(key, result.clone());
    result
}
