//! Typed errors: one variant per reason a solve did not produce a plan.

use thiserror::Error;

/// The configuration is unusable.  Mirrors the domain checks the CPython extension ran inside
/// `parse_config` (`_core.c:630-658`); the C core repeats them so a caller of the C ABI gets the
/// same refusal, but Rust reports them typed.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ConfigError {
    #[error("config value {field} is not finite")]
    NonFinite { field: &'static str },
    #[error("soc bounds do not produce a finite span ({e_min_kwh} .. {e_max_kwh})")]
    EnergyBounds { e_min_kwh: f64, e_max_kwh: f64 },
    #[error(
        "battery efficiencies must be positive (eta_charge {eta_charge}, eta_discharge {eta_discharge})"
    )]
    Efficiencies { eta_charge: f64, eta_discharge: f64 },
    #[error(
        "battery power limits must be non-negative (p_charge_kw {p_charge_kw}, p_discharge_kw {p_discharge_kw})"
    )]
    PowerLimits {
        p_charge_kw: f64,
        p_discharge_kw: f64,
    },
    #[error("slot_minutes must be non-negative, got {0}")]
    SlotMinutes(f64),
    #[error("power_steps {0} is outside the supported range 0..=1000000")]
    PowerSteps(i64),
    #[error("max_states {0} is outside the supported range 8..=1000000")]
    MaxStates(i64),
    #[error("max_iterations must be positive, got {0}")]
    MaxIterations(i64),
    #[error("{field} does not fit the C core's integer type")]
    CountTooLarge { field: &'static str },
}

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
