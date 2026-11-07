//! The solve entry points and their error mapping.

use super::{SolveInput, SolveResult, identity};
use crate::config::Config;
use crate::error::{InputError, SolveError};
use crate::ffi;
use crate::validate;

/// Why the C core refused the call, as a typed error.
fn core_error(code: i32, message: String) -> SolveError {
    match code {
        ffi::EO_ERR_TERMINAL_ROW => SolveError::TerminalRow(message),
        ffi::EO_ERR_ARGUMENT | ffi::EO_ERR_DOMAIN | ffi::EO_ERR_LIMITS | ffi::EO_ERR_GRID => {
            SolveError::Input(InputError::Rejected(message))
        }
        _ => SolveError::Core { code, message },
    }
}

/// Solve one scenario with the salvage terminal model.
pub fn solve(input: &SolveInput<'_>, config: &Config) -> Result<SolveResult, SolveError> {
    solve_inner(input, config, None)
}

/// Solve one scenario with an optimizer v2 stage-3 continuation row.
///
/// The row *is* the terminal condition (the caller's `dp._row_with_penalty` already carries the
/// target penalty); its length must equal the state grid the solver will build.
pub fn solve_with_terminal_row(
    input: &SolveInput<'_>,
    config: &Config,
    terminal_row: &[f64],
) -> Result<SolveResult, SolveError> {
    solve_inner(input, config, Some(terminal_row))
}

/// Reject a row that cannot be the terminal condition before the C call.
fn check_terminal_row(
    config: &Config,
    slots: usize,
    terminal_row: &[f64],
) -> Result<(), SolveError> {
    let (grid, _) = validate::reference_grid(config, slots);
    if terminal_row.len() != grid.len() {
        return Err(SolveError::TerminalRow(format!(
            "the row has {} entries for a {}-state grid",
            terminal_row.len(),
            grid.len()
        )));
    }
    if terminal_row.iter().any(|value| !value.is_finite()) {
        return Err(SolveError::TerminalRow("the row is not finite".into()));
    }
    Ok(())
}

fn solve_inner(
    input: &SolveInput<'_>,
    config: &Config,
    terminal_row: Option<&[f64]>,
) -> Result<SolveResult, SolveError> {
    identity::verify_core_build()?;
    config.validate()?;
    validate::check_input(input)?;
    if let Some(row) = terminal_row {
        check_terminal_row(config, input.len(), row)?;
    }
    let params = config.to_core_params()?;
    let owned = ffi::solve(&params, input, terminal_row)
        .map_err(|code| core_error(code, ffi::last_error()))?;
    let result = SolveResult::from_ffi(&owned);
    validate::check(&result, input, config, terminal_row)?;
    Ok(result)
}
