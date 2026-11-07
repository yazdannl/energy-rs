//! Typed errors: one variant per reason a solve did not produce a plan.
//!
//! Each kind lives in its own file -- a source file holds one primary type -- and this module is
//! only the wiring, so a caller keeps importing `energy_opt::error::{ConfigError, InputError,
//! SolveError, ValidationError}`.

mod config_error;
mod input_error;
mod solve_error;
mod validation_error;

pub use config_error::ConfigError;
pub use input_error::InputError;
pub use solve_error::SolveError;
pub use validation_error::ValidationError;
