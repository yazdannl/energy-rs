//! The periodic full differential against the brute-force oracle (`PLAN.md` §6).
//!
//! Every solve is already validated against a re-derivation of itself, but that shares the
//! solver's arithmetic. [`energy_opt::oracle`] shares nothing: it builds its own grid and sweeps
//! every state with a fresh action list. The engine re-solves one plan per interval with it and
//! compares the two results field by field. A disagreement is never tolerated -- the plan is
//! refused, which holds the packs, because a slightly wrong plan is worse than no plan.

use std::sync::atomic::{AtomicI64, Ordering};

use energy_opt::oracle::{self, OracleResult};
use energy_opt::{Config, SolveInput, SolveResult};

use crate::plan::PlanError;

/// The default schedule: one full differential per hour of engine time.
pub const DIFFERENTIAL_SECONDS: i64 = 3600;

/// The agreement bound, the same tolerance the parity fixtures pin the core to.
const TOLERANCE: f64 = 1e-9;

/// The engine's schedule for the slow oracle check.
///
/// The interval is a floor, not a promise: the check rides on a real solve, so it runs on the first
/// solve after the interval elapsed. `interval_s <= 0` checks every solve (tests, diagnostics).
#[derive(Debug)]
pub struct Differential {
    interval_s: i64,
    last: AtomicI64,
}

impl Default for Differential {
    fn default() -> Self {
        Self::new(DIFFERENTIAL_SECONDS)
    }
}

impl Differential {
    /// A differential that runs at most once every `interval_s` seconds.
    pub fn new(interval_s: i64) -> Self {
        Self {
            interval_s,
            last: AtomicI64::new(i64::MIN),
        }
    }

    /// Whether this solve is the one to check, claiming the slot so a concurrent solve skips it.
    pub fn due(&self, now: i64) -> bool {
        if self.interval_s <= 0 {
            return true;
        }
        let last = self.last.load(Ordering::Relaxed);
        if last != i64::MIN && now.saturating_sub(last) < self.interval_s {
            return false;
        }
        self.last
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Re-solve `input` naively and compare it with the core's `solved`.
    pub fn verify(
        &self,
        input: &SolveInput<'_>,
        config: &Config,
        solved: &SolveResult,
    ) -> Result<(), PlanError> {
        let reference = oracle::solve(config, input, None)
            .map_err(|error| refuse(format!("the oracle refused the scenario: {error}")))?;
        compare(solved, &reference)
    }
}

/// Compare a core result with the oracle's, refusing on the first disagreement.
fn compare(solved: &SolveResult, reference: &OracleResult) -> Result<(), PlanError> {
    if solved.states != reference.states {
        return Err(refuse(format!(
            "states: {} in the core, {} in the oracle",
            solved.states, reference.states
        )));
    }
    series("grid", &solved.grid, &reference.grid)?;
    series("value", &solved.value, &reference.value)?;
    series("energies", &solved.energies, &reference.energies)?;
    series("powers", &solved.powers, &reference.powers)?;
    scalar(
        "objective_eur",
        solved.objective_eur,
        reference.objective_eur,
    )?;
    scalar(
        "internal_objective_eur",
        solved.internal_objective_eur,
        reference.internal_objective_eur,
    )?;
    scalar(
        "cycle_cost_eur",
        solved.cycle_cost_eur,
        reference.cycle_cost_eur,
    )?;
    scalar(
        "terminal_error_kwh",
        solved.terminal_error_kwh,
        reference.terminal_error_kwh,
    )?;
    scalar(
        "effective_step_kwh",
        solved.effective_step_kwh,
        reference.effective_step_kwh,
    )
}

/// Compare one scalar with the tolerance the fixtures use.
fn scalar(name: &str, got: f64, want: f64) -> Result<(), PlanError> {
    if close(got, want) {
        return Ok(());
    }
    Err(refuse(format!(
        "{name}: {got} in the core, {want} in the oracle"
    )))
}

/// Compare one series element by element.
fn series(name: &str, got: &[f64], want: &[f64]) -> Result<(), PlanError> {
    if got.len() != want.len() {
        return Err(refuse(format!(
            "{name}: {} values in the core, {} in the oracle",
            got.len(),
            want.len()
        )));
    }
    for (index, (left, right)) in got.iter().zip(want).enumerate() {
        if !close(*left, *right) {
            return Err(refuse(format!(
                "{name}[{index}]: {left} in the core, {right} in the oracle"
            )));
        }
    }
    Ok(())
}

/// The absolute-or-relative agreement test the parity fixtures use.
fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= TOLERANCE + 1e-12 * want.abs()
}

/// The one refusal: a differential disagreement.
fn refuse(detail: String) -> PlanError {
    PlanError::Differential { detail }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use energy_opt::SolveResult;
    use energy_opt::oracle::OracleResult;

    use super::{Differential, compare};

    /// A result with two slots, one state grid point of 5 kWh and `objective` as its objective.
    fn solved(objective: f64) -> SolveResult {
        SolveResult {
            energies: vec![5.0, 4.0, 3.0],
            powers: vec![0.5, -0.25],
            windows: Vec::new(),
            grid: vec![3.0, 5.0],
            value: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            objective_eur: objective,
            cycle_cost_eur: 0.01,
            terminal_error_kwh: 0.0,
            effective_step_kwh: 2.0,
            internal_objective_eur: objective,
            iterations: 8,
            states: 2,
        }
    }

    /// The oracle's view of the same solve.
    fn reference(objective: f64) -> OracleResult {
        OracleResult {
            grid: vec![3.0, 5.0],
            value: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            argmin: vec![0.0; 4],
            energies: vec![5.0, 4.0, 3.0],
            powers: vec![0.5, -0.25],
            windows: Vec::new(),
            objective_eur: objective,
            cycle_cost_eur: 0.01,
            terminal_error_kwh: 0.0,
            effective_step_kwh: 2.0,
            internal_objective_eur: objective,
            iterations: 8,
            states: 2,
        }
    }

    #[test]
    fn identical_results_agree() {
        assert!(compare(&solved(1.5), &reference(1.5)).is_ok());
    }

    #[test]
    fn a_different_objective_is_refused() {
        let error = compare(&solved(1.5), &reference(1.6)).expect_err("objective drift");
        assert!(error.to_string().contains("objective_eur"), "{error}");
    }

    #[test]
    fn a_different_path_is_refused() {
        let mut oracle = reference(1.5);
        oracle.powers[1] = -0.26;
        let error = compare(&solved(1.5), &oracle).expect_err("path drift");
        assert!(error.to_string().contains("powers[1]"), "{error}");
    }

    #[test]
    fn a_different_grid_is_refused() {
        let mut oracle = reference(1.5);
        oracle.states = 3;
        let error = compare(&solved(1.5), &oracle).expect_err("grid drift");
        assert!(error.to_string().contains("states"), "{error}");
    }

    #[test]
    fn the_interval_is_a_floor() {
        let differential = Differential::new(3600);
        assert!(differential.due(1_000), "the first solve is checked");
        assert!(!differential.due(1_060), "the second one is too soon");
        assert!(differential.due(4_600), "an hour later it runs again");
    }

    #[test]
    fn a_zero_interval_checks_every_solve() {
        let differential = Differential::new(0);
        assert!(differential.due(1_000));
        assert!(differential.due(1_000));
    }
}
