//! The solver returned numbers that do not reproduce its own plan.

use thiserror::Error;

/// The solver returned numbers that do not reproduce its own plan.
///
/// Every variant is a refusal: the plan is never used when one of these fires (`PLAN.md` §6).
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValidationError {
    #[error("non-finite scalar: {0}")]
    NonFiniteScalar(&'static str),
    #[error("non-finite value in {0}")]
    NonFiniteArray(&'static str),
    #[error("{what} has {got} entries, expected {expected}")]
    Length {
        what: &'static str,
        expected: usize,
        got: usize,
    },
    #[error("the reported state count {states} does not match the returned grid ({grid})")]
    StateCount { states: usize, grid: usize },
    #[error("the state grid does not match the reference grid")]
    GridMismatch,
    #[error("the value function has the wrong length")]
    ValueLength,
    #[error("terminal row: {0}")]
    TerminalRow(&'static str),
    #[error("slot {slot}: windows do not match the reference windows")]
    WindowMismatch { slot: usize },
    #[error("slot {slot}: power {power} is outside the feasible bounds [{lo}, {hi}]")]
    Bounds {
        slot: usize,
        power: f64,
        lo: f64,
        hi: f64,
    },
    #[error("slot {slot}: power {power} is not the reference one-step argmin ({expected})")]
    Argmin {
        slot: usize,
        power: f64,
        expected: f64,
    },
    #[error("slot {slot}: the energy trajectory is not self-consistent")]
    PathEnergy { slot: usize },
    #[error("slot {slot}: the transition leaves [e_min, e_max]")]
    PathLeavesBounds { slot: usize },
    #[error("slot {slot}: the reference action set is empty or non-finite")]
    EmptyActionSet { slot: usize },
    #[error("objective {got} does not match the reconstructed path ({expected})")]
    Objective { expected: f64, got: f64 },
    #[error("cycle cost {got} does not match the accumulated wear ({expected})")]
    CycleCost { expected: f64, got: f64 },
    #[error("terminal error {got} does not match the reconstructed path ({expected})")]
    TerminalError { expected: f64, got: f64 },
    #[error("slot {slot}, state {state}: the value function fails backward consistency")]
    Backward { slot: usize, state: usize },
}
