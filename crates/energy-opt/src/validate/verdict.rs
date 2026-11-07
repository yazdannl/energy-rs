//! The verdict: shape, terminal row, path and backward consistency.

use super::backward::check_backward;
use super::cost::interp;
use super::grid::reference_grid;
use super::path::{PathChecker, PathTotals};
use super::{OBJECTIVE_ABS_TOL, OBJECTIVE_REL_TOL, VALUE_ABS_TOL, VALUE_REL_TOL, tol};
use crate::config::Config;
use crate::dp::{SolveInput, SolveResult};
use crate::error::ValidationError;

/// Verify a solver result against the reference arithmetic; see the module docs for the list.
///
/// `terminal_row` is the continuation row handed to the solver (`None` for the salvage model).
pub(crate) fn check(
    result: &SolveResult,
    input: &SolveInput<'_>,
    config: &Config,
    terminal_row: Option<&[f64]>,
) -> Result<(), ValidationError> {
    let states = check_shape(result, input, config)?;
    let row = check_terminal_row(result, config, states, terminal_row)?;
    let totals = PathChecker::new(result, input, config).totals()?;
    check_objective(result, config, &row, &totals, terminal_row.is_some())?;
    check_backward(result, input, config, terminal_row.is_some())
}

/// Lengths, state count and the state grid must be the reference's, or every window and
/// interpolation below is on a different discretisation than the sweep used.
pub(crate) fn check_shape(
    result: &SolveResult,
    input: &SolveInput<'_>,
    config: &Config,
) -> Result<usize, ValidationError> {
    let n = input.len();
    let states = result.states;
    if result.energies.len() != n || result.powers.len() != n || result.windows.len() != n {
        return Err(ValidationError::Length {
            what: "the result path",
            expected: n,
            got: result.energies.len(),
        });
    }
    if result.iterations < 0 || states < 1 || result.grid.len() != states {
        return Err(ValidationError::StateCount {
            states,
            grid: result.grid.len(),
        });
    }
    check_finite(result, config)?;
    let (grid, reference_step) = reference_grid(config, n);
    if grid.len() != states
        || (result.effective_step_kwh - reference_step).abs()
            > tol(VALUE_ABS_TOL, VALUE_REL_TOL, reference_step)
    {
        return Err(ValidationError::GridMismatch);
    }
    for (got, expected) in result.grid.iter().zip(grid.iter()) {
        if (got - expected).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, *expected) {
            return Err(ValidationError::GridMismatch);
        }
    }
    if result.value.len() != (n + 1) * states {
        return Err(ValidationError::ValueLength);
    }
    Ok(states)
}

/// Every scalar and every element of every array must be finite, `NaN` included.
fn check_finite(result: &SolveResult, config: &Config) -> Result<(), ValidationError> {
    for (name, value) in [
        ("effective_step_kwh", result.effective_step_kwh),
        ("terminal_error_kwh", result.terminal_error_kwh),
        ("cycle_cost_eur", result.cycle_cost_eur),
        ("objective_eur", result.objective_eur),
        ("internal_objective_eur", result.internal_objective_eur),
        ("start_kwh", config.start_kwh),
        ("salvage", config.salvage),
        ("target_penalty", config.target_penalty),
    ] {
        if !value.is_finite() {
            return Err(ValidationError::NonFiniteScalar(name));
        }
    }
    for (name, series) in [
        ("energies", result.energies.as_slice()),
        ("powers", result.powers.as_slice()),
        ("grid", result.grid.as_slice()),
        ("the value function", result.value.as_slice()),
    ] {
        if series.iter().any(|value| !value.is_finite()) {
            return Err(ValidationError::NonFiniteArray(name));
        }
    }
    if result
        .windows
        .iter()
        .any(|window| !window.values().iter().all(|value| value.is_finite()))
    {
        return Err(ValidationError::NonFiniteArray("windows"));
    }
    Ok(())
}

/// The terminal row: a supplied row must appear verbatim (it is copied, not recomputed); without
/// one the salvage row is re-derived.  `NaN` fails closed before any tolerance comparison.
#[allow(clippy::float_cmp)] // a copied row is compared bit for bit on purpose
pub(crate) fn check_terminal_row(
    result: &SolveResult,
    config: &Config,
    states: usize,
    terminal_row: Option<&[f64]>,
) -> Result<Vec<f64>, ValidationError> {
    let offset = result.powers.len() * states;
    let Some(supplied) = terminal_row else {
        return derive_terminal_row(result, config, states);
    };
    if supplied.len() != states {
        return Err(ValidationError::TerminalRow(
            "the row length does not match the state grid",
        ));
    }
    if supplied.iter().any(|entry| !entry.is_finite()) {
        return Err(ValidationError::TerminalRow("the row is not finite"));
    }
    for (k, want) in supplied.iter().enumerate() {
        if result.value[offset + k] != *want {
            return Err(ValidationError::TerminalRow(
                "the terminal row does not match the supplied row",
            ));
        }
    }
    Ok(supplied.to_vec())
}

/// The salvage row the core must have built, checked against the reported last row.
fn derive_terminal_row(
    result: &SolveResult,
    config: &Config,
    states: usize,
) -> Result<Vec<f64>, ValidationError> {
    let offset = result.powers.len() * states;
    let mut row = Vec::with_capacity(states);
    for (k, &e) in result.grid.iter().enumerate() {
        let mut expected = -config.salvage * e;
        if let Some(target) = config.e_target {
            expected += config.target_penalty * (e - target).abs();
        }
        let got = result.value[offset + k];
        if !(got.is_finite() && expected.is_finite()) {
            return Err(ValidationError::TerminalRow(
                "the terminal row is not finite",
            ));
        }
        if (got - expected).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, expected) {
            return Err(ValidationError::TerminalRow(
                "the terminal row does not match the reference",
            ));
        }
        row.push(expected);
    }
    Ok(row)
}

/// The objective, cycle cost and terminal error, re-derived from the reconstructed path.
fn check_objective(
    result: &SolveResult,
    config: &Config,
    row: &[f64],
    totals: &PathTotals,
    has_terminal_row: bool,
) -> Result<(), ValidationError> {
    if (result.cycle_cost_eur - totals.wear).abs()
        > tol(OBJECTIVE_ABS_TOL, OBJECTIVE_REL_TOL, totals.wear)
    {
        return Err(ValidationError::CycleCost {
            expected: totals.wear,
            got: result.cycle_cost_eur,
        });
    }
    let mut objective = totals.objective + totals.wear;
    if has_terminal_row {
        objective += interp(row, &result.grid, totals.e_final);
    } else {
        objective -= config.salvage * totals.e_final;
    }
    let mut expected_terminal = 0.0;
    if let Some(target) = config.e_target {
        expected_terminal = (totals.e_final - target).abs();
        if !has_terminal_row {
            objective += config.target_penalty * expected_terminal;
        }
    }
    if !objective.is_finite() {
        return Err(ValidationError::Objective {
            expected: objective,
            got: result.objective_eur,
        });
    }
    let tolerance = OBJECTIVE_ABS_TOL + OBJECTIVE_REL_TOL * objective.abs();
    if (objective - result.objective_eur).abs() > tolerance {
        return Err(ValidationError::Objective {
            expected: objective,
            got: result.objective_eur,
        });
    }
    if (expected_terminal - result.terminal_error_kwh).abs() > tolerance {
        return Err(ValidationError::TerminalError {
            expected: expected_terminal,
            got: result.terminal_error_kwh,
        });
    }
    Ok(())
}
