//! Why a solve produced no plan.

use thiserror::Error;

use super::{ConfigError, InputError, ValidationError};

/// Why a solve produced no plan.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SolveError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Input(#[from] InputError),
    #[error("the terminal row is unusable: {0}")]
    TerminalRow(String),
    #[error("the solver result was refused: {0}")]
    Refused(#[from] ValidationError),
    #[error("the C core failed ({code}): {message}")]
    Core { code: i32, message: String },
    #[error(
        "the C core was built from different sources (artifact {built}, sources {expected}); rebuild the crate"
    )]
    StaleCore { built: String, expected: String },
}
