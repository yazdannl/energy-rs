//! The reference solve: build the grid, sweep, replay, account for the terminal row.

use super::arithmetic::clamp;
use super::grid::state_grid;
use super::replay::{Path, finish, reconstruct};
use super::result::OracleResult;
use super::sweep::sweep;
use crate::config::Config;
use crate::dp::SolveInput;
use crate::error::SolveError;

/// The terminal row the sweep starts from (the supplied one, or the salvage row).
fn terminal_values(config: &Config, grid: &[f64], terminal_row: Option<&[f64]>) -> Vec<f64> {
    grid.iter()
        .enumerate()
        .map(|(state, energy)| {
            if let Some(row) = terminal_row {
                return row[state];
            }
            let mut entry = -config.salvage * energy;
            if let Some(target) = config.e_target {
                entry += config.target_penalty * (energy - target).abs();
            }
            entry
        })
        .collect()
}

/// The grid point closest to the start state (first wins on a tie).
fn nearest_index(grid: &[f64], start: f64) -> usize {
    let mut index = 0usize;
    for state in 1..grid.len() {
        if (grid[state] - start).abs() < (grid[index] - start).abs() {
            index = state;
        }
    }
    index
}

/// Run the reference DP over `input`.
pub fn solve(
    config: &Config,
    input: &SolveInput<'_>,
    terminal_row: Option<&[f64]>,
) -> Result<OracleResult, SolveError> {
    config.validate()?;
    crate::validate::check_input(input)?;
    let (grid, step) = state_grid(config, input.len());
    let states = grid.len();
    if let Some(row) = terminal_row
        && row.len() != states
    {
        return Err(SolveError::TerminalRow(format!(
            "the row has {} entries for a {states}-state grid",
            row.len()
        )));
    }
    let terminal = terminal_values(config, &grid, terminal_row);
    let (value, argmin, iterations) = sweep(config, input, &grid, &terminal);
    let layers = super::replay::Layers::new(&grid, &value, &argmin, input.len());
    let path: Path = reconstruct(config, input, &layers);
    let (objective, terminal_error) = finish(config, terminal_row, &grid, &path);
    let internal = value[nearest_index(
        &grid,
        clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh),
    )];
    Ok(OracleResult {
        grid,
        value,
        argmin,
        energies: path.energies,
        powers: path.powers,
        windows: path.windows,
        objective_eur: objective,
        cycle_cost_eur: path.cycle_cost,
        terminal_error_kwh: terminal_error,
        effective_step_kwh: step,
        internal_objective_eur: internal,
        iterations,
        states,
    })
}
