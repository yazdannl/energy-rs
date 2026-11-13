//! The path: the stored decisions replayed from the start state.

use super::arithmetic::{actions, clamp, exact_power, interp, slot_cost, transition, wear};
use super::sample::SlotSample;
use super::windows::{feasible, grid_window, soc_window};
use crate::config::Config;
use crate::dp::SolveInput;

/// The replayed path and the costs the reference accumulates over it.
pub(crate) struct Path {
    pub(crate) energies: Vec<f64>,
    pub(crate) powers: Vec<f64>,
    pub(crate) windows: Vec<[f64; 4]>,
    pub(crate) cycle_cost: f64,
    pub(crate) objective: f64,
    pub(crate) final_energy: f64,
}

/// The value function and the stored choices, addressed by `(slot, state)`.
pub(crate) struct Layers<'a> {
    grid: &'a [f64],
    value: &'a [f64],
    argmin: &'a [f64],
    states: usize,
    slots: usize,
}

impl<'a> Layers<'a> {
    pub(crate) fn new(grid: &'a [f64], value: &'a [f64], argmin: &'a [f64], slots: usize) -> Self {
        Self {
            grid,
            value,
            argmin,
            states: grid.len(),
            slots,
        }
    }

    fn next_row(&self, index: usize) -> &[f64] {
        &self.value[(index + 1) * self.states..(index + 2) * self.states]
    }

    fn chosen(&self, index: usize, state: usize) -> f64 {
        self.argmin[index * self.states + state]
    }
}

/// Replay the stored decisions from the clamped start state.
pub(crate) fn reconstruct(config: &Config, input: &SolveInput<'_>, layers: &Layers<'_>) -> Path {
    let mut energies = Vec::with_capacity(layers.slots);
    let mut powers = Vec::with_capacity(layers.slots);
    let mut windows = Vec::with_capacity(layers.slots);
    let mut cycle_cost = 0.0f64;
    let mut objective = 0.0f64;
    let mut energy = clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh);
    for index in 0..layers.slots {
        let sample = SlotSample::at(input, index);
        let dt = sample.dt();
        let (lo, hi) = feasible(config, &sample, energy, dt);
        let (soc_lo, soc_hi) = soc_window(config, energy, dt);
        let (grid_lo, grid_hi) = grid_window(config, &sample);
        windows.push([soc_lo, soc_hi, grid_lo, grid_hi]);
        let power = chosen_power(config, &sample, layers, index, energy, (lo, hi));
        let next = transition(config, energy, power, dt);
        energies.push(energy);
        powers.push(power);
        cycle_cost += wear(config, energy, next);
        objective += slot_cost(config, &sample, power);
        energy = clamp(next, config.e_min_kwh, config.e_max_kwh);
    }
    Path {
        energies,
        powers,
        windows,
        cycle_cost,
        objective,
        final_energy: energy,
    }
}

/// The stored action at `energy`, or a fresh action loop when the state is off-grid.
fn chosen_power(
    config: &Config,
    sample: &SlotSample,
    layers: &Layers<'_>,
    index: usize,
    energy: f64,
    window: (f64, f64),
) -> f64 {
    let (lo, hi) = window;
    if let Some(state) = layers
        .grid
        .iter()
        .position(|point| (*point - energy).abs() <= 1e-9)
    {
        return clamp(layers.chosen(index, state), lo, hi);
    }
    let dt = sample.dt();
    let next_row = layers.next_row(index);
    let extra = match (index + 1 == layers.slots, config.e_target) {
        (true, Some(target)) => Some(exact_power(config, energy, target, dt)),
        _ => None,
    };
    let align = config
        .align_actions
        .then(|| (sample.surplus(), -sample.base()));
    let mut best = f64::INFINITY;
    let mut choice = clamp(0.0, lo, hi);
    for power in actions(lo, hi, config.power_steps, extra, align) {
        let next = transition(config, energy, power, dt);
        let cost = slot_cost(config, sample, power)
            + wear(config, energy, next)
            + interp(next_row, layers.grid, next);
        if cost < best {
            best = cost;
            choice = power;
        }
    }
    clamp(choice, lo, hi)
}

/// The terminal accounting: the objective and `|final SoC - e_target|`.
pub(crate) fn finish(
    config: &Config,
    terminal_row: Option<&[f64]>,
    grid: &[f64],
    path: &Path,
) -> (f64, f64) {
    let mut objective = path.objective + path.cycle_cost;
    if let Some(row) = terminal_row {
        objective += interp(row, grid, path.final_energy);
    } else {
        objective -= config.salvage * path.final_energy;
        if let Some(target) = config.e_target {
            objective += config.target_penalty * (path.final_energy - target).abs();
        }
    }
    let terminal_error = match config.e_target {
        Some(target) => (path.final_energy - target).abs(),
        None => 0.0,
    };
    (objective, terminal_error)
}
