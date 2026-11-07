//! Re-derive the plan's path, per-slot decisions and costs.

use super::cost::{clamp, exact_power, interp, next_energy, slot_cost, wear_cost};
use super::one_step::{SlotContext, argmin_at};
use super::slot::{Slot, slot_at};
use super::window::{bounds, grid_window, soc_window};
use super::{DECISION_ABS_TOL, ENERGY_TOL, VALUE_ABS_TOL, VALUE_REL_TOL, tol, tol_kwh};
use crate::config::Config;
use crate::dp::{SolveInput, SolveResult};
use crate::error::ValidationError;

/// What the path re-derivation accumulated.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PathTotals {
    /// The stored energy after the last slot, kWh (clamped to the pack).
    pub e_final: f64,
    /// The sum of the slot costs over the path, EUR.
    pub objective: f64,
    /// The sum of the transition wear over the path, EUR.
    pub wear: f64,
}

/// Re-derives the path against the reference arithmetic, slot by slot.
pub(crate) struct PathChecker<'a, 'b> {
    result: &'a SolveResult,
    input: &'a SolveInput<'b>,
    config: &'a Config,
}

impl<'a, 'b> PathChecker<'a, 'b> {
    /// A checker for one result and the scenario it claims to solve.
    pub(crate) fn new(
        result: &'a SolveResult,
        input: &'a SolveInput<'b>,
        config: &'a Config,
    ) -> Self {
        Self {
            result,
            input,
            config,
        }
    }

    /// Walk the whole path; any mismatch is a refusal.
    pub(crate) fn totals(&self) -> Result<PathTotals, ValidationError> {
        let config = self.config;
        let mut e_now = clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh);
        let mut totals = PathTotals {
            e_final: e_now,
            objective: 0.0,
            wear: 0.0,
        };
        let tolerance = tol_kwh(config);
        for index in 0..self.input.len() {
            let (e_next, step, wear) = self.slot(index, e_now)?;
            totals.objective += step;
            totals.wear += wear;
            if !(totals.objective.is_finite() && totals.wear.is_finite()) {
                return Err(ValidationError::Objective {
                    expected: totals.objective,
                    got: self.result.objective_eur,
                });
            }
            if e_next < config.e_min_kwh - tolerance || e_next > config.e_max_kwh + tolerance {
                return Err(ValidationError::PathLeavesBounds { slot: index });
            }
            e_now = clamp(e_next, config.e_min_kwh, config.e_max_kwh);
        }
        totals.e_final = e_now;
        Ok(totals)
    }

    // A decision is a discrete choice, not a measurement: the reference argmin must be
    // reproduced exactly, so this is the one comparison in the crate that is intentionally `!=`.
    #[allow(clippy::float_cmp)]
    fn slot(&self, index: usize, e_now: f64) -> Result<(f64, f64, f64), ValidationError> {
        let slot = slot_at(self.input, index);
        let dt = slot.dt();
        if (self.result.energies[index] - e_now).abs() > tol_kwh(self.config) {
            return Err(ValidationError::PathEnergy { slot: index });
        }
        let (lo, hi) = self.windows(index, &slot, e_now, dt)?;
        let power = self.result.powers[index];
        if power < lo - DECISION_ABS_TOL || power > hi + DECISION_ABS_TOL {
            return Err(ValidationError::Bounds {
                slot: index,
                power,
                lo,
                hi,
            });
        }
        let expected = self.expected_power(index, &slot, e_now, dt, (lo, hi))?;
        if power != expected {
            return Err(ValidationError::Argmin {
                slot: index,
                power,
                expected,
            });
        }
        self.costs(index, &slot, e_now, power, dt)
    }

    /// Check the reported windows and return the intersected feasible window.
    fn windows(
        &self,
        index: usize,
        slot: &Slot,
        e_now: f64,
        dt: f64,
    ) -> Result<(f64, f64), ValidationError> {
        let (soc_lo, soc_hi) = soc_window(self.config, e_now, dt);
        let (grid_lo, grid_hi) = grid_window(self.config, slot);
        let got = self.result.windows[index];
        for (value, expected) in [
            (got.soc_lo_kw, soc_lo),
            (got.soc_hi_kw, soc_hi),
            (got.grid_lo_kw, grid_lo),
            (got.grid_hi_kw, grid_hi),
        ] {
            if (value - expected).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, expected) {
                return Err(ValidationError::WindowMismatch { slot: index });
            }
        }
        Ok(bounds(self.config, slot, e_now, dt))
    }

    /// The power the reference strict argmin chooses at the grid point the state snaps to.
    fn expected_power(
        &self,
        index: usize,
        slot: &Slot,
        e_now: f64,
        dt: f64,
        window: (f64, f64),
    ) -> Result<f64, ValidationError> {
        let grid = &self.result.grid;
        let (lo, hi) = window;
        let mut state = e_now;
        let mut action_lo = lo;
        let mut action_hi = hi;
        // The stored decision was taken at the grid point the state snaps to, with that point's
        // bounds; the reconstruction clamp below uses this slot's own bounds. Both are reproduced.
        if let Some(point) = grid
            .iter()
            .find(|point| (**point - e_now).abs() <= ENERGY_TOL)
        {
            state = *point;
            (action_lo, action_hi) = bounds(self.config, slot, state, dt);
        }
        let extra = match (index + 1 == self.input.len(), self.config.e_target) {
            (true, Some(target)) => Some(exact_power(self.config, state, target, dt)),
            _ => None,
        };
        let ctx = SlotContext::new(
            self.config,
            *slot,
            dt,
            self.result.value_row(index + 1),
            grid,
        );
        let (best_power, _cost) = argmin_at(&ctx, state, action_lo, action_hi, extra)
            .ok_or(ValidationError::EmptyActionSet { slot: index })?;
        Ok(clamp(best_power, lo, hi))
    }

    /// The transition, its grid cost and its wear; all three must be finite.
    fn costs(
        &self,
        index: usize,
        slot: &Slot,
        e_now: f64,
        power: f64,
        dt: f64,
    ) -> Result<(f64, f64, f64), ValidationError> {
        let e_next = next_energy(self.config, e_now, power, dt);
        let step_cost = slot_cost(self.config, slot, dt, power);
        let wear = wear_cost(self.config, e_now, e_next);
        let future = interp(self.result.value_row(index + 1), &self.result.grid, e_next);
        let total_is_finite = (step_cost + wear + future).is_finite();
        if !(e_next.is_finite() && step_cost.is_finite() && wear.is_finite()) {
            return Err(ValidationError::PathEnergy { slot: index });
        }
        if !(future.is_finite() && total_is_finite) {
            return Err(ValidationError::PathEnergy { slot: index });
        }
        Ok((e_next, step_cost, wear))
    }
}
