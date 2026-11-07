//! The validated solve result.

use super::Window;
use crate::ffi;

/// One validated solve; mirrors the old core's result dict.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveResult {
    /// Stored energy at the start of each slot, kWh.
    pub energies: Vec<f64>,
    /// Battery power per slot, kW (positive = charging).
    pub powers: Vec<f64>,
    /// The SoC and grid windows per slot.
    pub windows: Vec<Window>,
    /// The state grid, ascending, kWh.
    pub grid: Vec<f64>,
    /// The value function, `(slots + 1) * states` row-major; row `slots` is the terminal row.
    pub value: Vec<f64>,
    /// The exact value the solver minimised, EUR.
    pub objective_eur: f64,
    /// The wear accumulated over the returned path, EUR.
    pub cycle_cost_eur: f64,
    /// `|final SoC - e_target|`, kWh; zero when no target was set.
    pub terminal_error_kwh: f64,
    /// The step the grid really used (coarsened when the state budget tightened it), kWh.
    pub effective_step_kwh: f64,
    /// The value function's entry at the start state, EUR.
    pub internal_objective_eur: f64,
    /// Actions evaluated by the sweep.
    pub iterations: i64,
    /// Grid points in the state grid.
    pub states: usize,
}

impl SolveResult {
    /// Row `index` of the value function (`index == slots` is the terminal row).
    pub fn value_row(&self, index: usize) -> &[f64] {
        &self.value[index * self.states..(index + 1) * self.states]
    }

    /// The terminal row the sweep started from.
    pub fn terminal_row(&self) -> &[f64] {
        &self.value[self.powers.len() * self.states..]
    }

    pub(crate) fn from_ffi(owned: &ffi::OwnedResult) -> Self {
        let windows = owned
            .windows()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|window| Window {
                soc_lo_kw: window[0],
                soc_hi_kw: window[1],
                grid_lo_kw: window[2],
                grid_hi_kw: window[3],
            })
            .collect();
        Self {
            energies: owned.energies().to_vec(),
            powers: owned.powers().to_vec(),
            windows,
            grid: owned.grid().to_vec(),
            value: owned.value().to_vec(),
            objective_eur: owned.0.objective_eur,
            cycle_cost_eur: owned.0.cycle_cost_eur,
            terminal_error_kwh: owned.0.terminal_error_kwh,
            effective_step_kwh: owned.0.effective_step_kwh,
            internal_objective_eur: owned.0.internal_objective_eur,
            iterations: owned.0.iterations,
            states: usize::try_from(owned.0.states).unwrap_or(0),
        }
    }
}
