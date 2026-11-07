//! Validate the scenario arrays before any solve.

use crate::dp::SolveInput;
use crate::error::InputError;

/// Validate the scenario arrays: equal lengths, every value finite, within the slot ceiling.
pub(crate) fn check_input(input: &SolveInput<'_>) -> Result<(), InputError> {
    let n = input.len();
    if n > crate::ffi::EO_MAX_SLOTS {
        return Err(InputError::TooManySlots(n));
    }
    for (name, series) in [
        ("load_kw", input.load_kw),
        ("buy", input.buy),
        ("sell", input.sell),
        ("minutes", input.minutes),
    ] {
        if series.len() != n {
            return Err(InputError::Length {
                name,
                expected: n,
                got: series.len(),
            });
        }
    }
    for (name, series) in [
        ("pv_kw", input.pv_kw),
        ("load_kw", input.load_kw),
        ("buy", input.buy),
        ("sell", input.sell),
        ("minutes", input.minutes),
    ] {
        for (index, value) in series.iter().enumerate() {
            if !value.is_finite() {
                return Err(InputError::NonFinite { name, index });
            }
        }
    }
    Ok(())
}
