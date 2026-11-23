//! One action choice: exact replayed cost plus the canonical key of the final path.

use crate::optimizer::context::ChooseContext;
use crate::optimizer::continuation::Continuation;
use crate::optimizer::feasible;
use crate::optimizer::key::{CanonicalKey, path_key};
use crate::optimizer::objective::{quantise_wh, wear_eur};
use crate::optimizer::{BindingBound, OptimizerRefusal};

/// Finite stand-in for a continuation that cannot meet any physical bound.
pub(crate) const INFEASIBLE: f64 = 1.0e9;

/// Choose the canonical best action from an exact stored energy. Every candidate is a concrete
/// replayed path, so the key used here already describes what will be executed.
pub(crate) fn best_action(
    context: &ChooseContext<'_>,
    energy: f64,
    refused: &mut Vec<BindingBound>,
) -> Result<(f64, Continuation), OptimizerRefusal> {
    let index = context.index;
    let mut best: Option<(f64, Continuation, CanonicalKey)> = None;
    for action in &context.evals_by_slot[index] {
        let Some(next) = feasible::next_energy(
            context.input,
            index,
            energy,
            action.power_w,
            &context.evals_by_slot[index],
        ) else {
            let mut bounds = feasible::rejection_bounds(context.input, index, action.power_w);
            if bounds.is_empty() {
                bounds.push(BindingBound::new(
                    "pack_power",
                    index,
                    f64::from(action.power_w).abs() / 1000.0,
                ));
            }
            for bound in bounds {
                feasible::push_bound(refused, bound);
            }
            continue;
        };
        let continuation = crate::optimizer::continuation::best(context, next);
        if continuation.cost_eur >= INFEASIBLE / 2.0 {
            continue;
        }
        let candidate = make_candidate(context, energy, action, next, &continuation);
        if best
            .as_ref()
            .is_none_or(|(_, _, incumbent)| candidate.2 < *incumbent)
        {
            best = Some(candidate);
        }
    }
    best.map(|(cost, continuation, _)| (cost, continuation))
        .ok_or(OptimizerRefusal::Infeasible {
            slot: index,
            binding_bounds: refused.clone(),
        })
}

fn make_candidate(
    context: &ChooseContext<'_>,
    energy: f64,
    action: &crate::optimizer::dp::ActionEval,
    next: f64,
    continuation: &Continuation,
) -> (f64, Continuation, CanonicalKey) {
    let config = &context.input.config;
    let immediate = action.grid_eur + wear_eur(energy, next, config.wear_eur_per_stored_kwh);
    let cost = immediate + continuation.cost_eur;
    let first = if action.power_w < 0 {
        context.index
    } else {
        continuation.first_discharge
    };
    let squared =
        i64::from(action.power_w) * i64::from(action.power_w) + continuation.power_squared_w2;
    let nonzero = usize::from(action.power_w != 0) + continuation.nonzero_slots;
    let mut energy_sig = Vec::with_capacity(continuation.energy_signature.len() + 1);
    energy_sig.push(quantise_wh(next));
    energy_sig.extend_from_slice(&continuation.energy_signature);
    let mut power_sig = Vec::with_capacity(continuation.power_signature.len() + 1);
    power_sig.push(action.power_w);
    power_sig.extend_from_slice(&continuation.power_signature);
    let mut powers = Vec::with_capacity(continuation.powers.len() + 1);
    powers.push(action.power_w);
    powers.extend_from_slice(&continuation.powers);
    let key = path_key(cost, first, squared, nonzero, &energy_sig, &power_sig);
    (
        cost,
        Continuation {
            cost_eur: cost,
            first_discharge: first,
            power_squared_w2: squared,
            nonzero_slots: nonzero,
            energy_signature: energy_sig,
            power_signature: power_sig,
            powers,
        },
        key,
    )
}
