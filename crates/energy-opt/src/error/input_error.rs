//! The scenario arrays are unusable.

use thiserror::Error;

/// The scenario arrays are unusable.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum InputError {
    #[error("{name} has {got} entries but pv_kw has {expected}")]
    Length {
        name: &'static str,
        expected: usize,
        got: usize,
    },
    #[error("{name}[{index}] is not finite")]
    NonFinite { name: &'static str, index: usize },
    #[error("the scenario has {0} slots; the core accepts at most 1000000")]
    TooManySlots(usize),
    #[error("the core rejected the scenario: {0}")]
    Rejected(String),
}
