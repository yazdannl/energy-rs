//! The backward sweep: every state, every action, in the most direct way.

use super::arithmetic::{actions, clamp, exact_power, interp, slot_cost, transition, wear};
use super::sample::SlotSample;
use super::windows::feasible;
use crate::config::Config;
use crate::dp::SolveInput;

/// One state at one slot: everything the action loop reads.
struct Step<'a> {
    config: &'a Config,
    sample: &'a SlotSample,
    grid: &'a [f64],
    next_row: &'a [f64],
    dt: f64,
    last: bool,
}

impl Step<'_> {
    /// The best total cost at `energy`, the action that achieves it, and actions evaluated.
    fn best(&self, energy: f64) -> (f64, f64, i64) {
        let (lo, hi) = feasible(self.config, self.sample, energy, self.dt);
        let extra = match (self.last, self.config.e_target) {
            (true, Some(target)) => Some(exact_power(self.config, energy, target, self.dt)),
            _ => None,
        };
        let mut best = f64::INFINITY;
        let mut choice = clamp(0.0, lo, hi);
        let mut iterations = 0i64;
        for power in actions(lo, hi, self.config.power_steps, extra) {
            let next = transition(self.config, energy, power, self.dt);
            let cost = slot_cost(self.config, self.sample, power)
                + wear(self.config, energy, next)
                + interp(self.next_row, self.grid, next);
            iterations += 1;
            if cost < best {
                best = cost;
                choice = power;
            }
        }
        (best, choice, iterations)
    }
}

/// Fill the value function and the stored argmin; returns both and the action count.
pub(crate) fn sweep(
    config: &Config,
    input: &SolveInput<'_>,
    grid: &[f64],
    terminal: &[f64],
) -> (Vec<f64>, Vec<f64>, i64) {
    let slots = input.len();
    let states = grid.len();
    let mut value = vec![0.0f64; (slots + 1) * states];
    value[slots * states..].copy_from_slice(terminal);
    let mut argmin = vec![0.0f64; slots * states];
    let mut iterations = 0i64;
    for index in (0..slots).rev() {
        let sample = SlotSample::at(input, index);
        let next_row = value[(index + 1) * states..(index + 2) * states].to_vec();
        let mut row = vec![0.0f64; states];
        let mut choices = vec![0.0f64; states];
        let step = Step {
            config,
            sample: &sample,
            grid,
            next_row: &next_row,
            dt: sample.dt(),
            last: index + 1 == slots,
        };
        for (state, energy) in grid.iter().enumerate() {
            let (best, choice, evaluated) = step.best(*energy);
            row[state] = best;
            choices[state] = choice;
            iterations += evaluated;
        }
        value[index * states..(index + 1) * states].copy_from_slice(&row);
        argmin[index * states..(index + 1) * states].copy_from_slice(&choices);
    }
    (value, argmin, iterations)
}
