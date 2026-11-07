//! The sampled backward Bellman pass over the value function.

use super::cost::exact_power;
use super::one_step::{SlotContext, argmin_at};
use super::slot::{Slot, slot_at};
use super::window::bounds;
use super::{VALUE_ABS_TOL, VALUE_REL_TOL, tol};
use crate::config::Config;
use crate::dp::{SolveInput, SolveResult};
use crate::error::ValidationError;

/// Python's `round()` (half to even) applied to the sampled indices of `_fast._sample_indices`.
fn sample_indices(total: usize, want: usize) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    if total <= want {
        return (0..total).collect();
    }
    let mut out: Vec<usize> = (0..want)
        .map(|i| {
            let raw = i as f64 * (total - 1) as f64 / (want - 1) as f64;
            // `raw` lies in `[0, total - 1]`: a non-negative index by construction.
            #[allow(clippy::cast_sign_loss)]
            let index = raw.round_ties_even() as usize;
            index.min(total - 1)
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Sampled backward consistency: `V_i[k]` must be the reference one-step Bellman value of
/// `V_(i+1)`.  With a terminal row the whole last row is recomputed, because that row is the one
/// place the supplied row binds the sweep; elsewhere five slots and five states are sampled and
/// the periodic full differential bounds the rest.
pub(crate) fn check_backward(
    result: &SolveResult,
    input: &SolveInput<'_>,
    config: &Config,
    has_terminal_row: bool,
) -> Result<(), ValidationError> {
    let states = result.states;
    for index in sample_indices(input.len(), 5) {
        let slot = slot_at(input, index);
        let states_to_check = if has_terminal_row && index + 1 == input.len() {
            (0..states).collect()
        } else {
            sample_indices(states, 5)
        };
        for state in states_to_check {
            check_state(result, config, &slot, index, state)?;
        }
    }
    Ok(())
}

/// One `(slot, state)` entry against the reference one-step Bellman value.
fn check_state(
    result: &SolveResult,
    config: &Config,
    slot: &Slot,
    index: usize,
    state: usize,
) -> Result<(), ValidationError> {
    let dt = slot.dt();
    let e = result.grid[state];
    let (lo, hi) = bounds(config, slot, e, dt);
    let extra = match (index + 1 == result.powers.len(), config.e_target) {
        (true, Some(target)) => Some(exact_power(config, e, target, dt)),
        _ => None,
    };
    let ctx = SlotContext::new(config, *slot, dt, result.value_row(index + 1), &result.grid);
    let (_, best_cost) =
        argmin_at(&ctx, e, lo, hi, extra).ok_or(ValidationError::EmptyActionSet { slot: index })?;
    let got = result.value[index * result.states + state];
    if !(got.is_finite() && best_cost.is_finite()) {
        return Err(ValidationError::Backward { slot: index, state });
    }
    if (got - best_cost).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, best_cost) {
        return Err(ValidationError::Backward { slot: index, state });
    }
    Ok(())
}
