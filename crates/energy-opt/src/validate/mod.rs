//! Input validation and post-solve verification for the DP (`PLAN.md` §6).
//!
//! Two jobs:
//!
//! 1. **The reference arithmetic.** [`clamp`], [`next_energy`], [`soc_window`], [`grid_window`],
//!    [`bounds`], [`actions`], [`interp`], [`slot_cost`], [`wear_cost`], [`exact_power`] and
//!    [`build_grid`] mirror `dp.py` operation for operation (same expression order, same
//!    tie-breaks).  They are what the checks below re-derive the plan from, and what a caller can
//!    use to reason about a slot without going through C.
//! 2. **The verdict.** [`check`] is a port of `_fast._validated`: structure, lengths and
//!    finiteness of every number including the whole value buffer; the state grid rebuilt and
//!    compared; every power inside the re-derived bounds; every per-slot decision equal to the
//!    reference's strict argmin; the objective, terminal error and cycle cost re-derived from the
//!    reconstructed path; and a sampled backward Bellman pass over the value function (the whole
//!    last row when a terminal row was supplied).  Any mismatch is a refusal, never "close
//!    enough" -- an incorrect plan must not reach a battery.
//!
//! Each aspect lives in its own file: arithmetic (`cost`, `window`, `grid`), the inputs
//! (`slot`, `input`), the one-step reference (`one_step`), and the verdict (`verdict`, `path`,
//! `backward`).  This module is the wiring plus the tolerances every aspect shares.

mod backward;
mod cost;
mod grid;
mod input;
mod one_step;
mod path;
mod slot;
mod verdict;
mod window;

pub use cost::{clamp, exact_power, interp, next_energy, slot_cost, wear_cost};
pub use grid::{build_grid, reference_grid};
pub use one_step::{SlotContext, argmin_at};
pub use slot::Slot;
pub use window::{actions, bounds, grid_window, soc_window};

pub(crate) use input::check_input;
pub(crate) use verdict::check;

/// Power tolerance for "this window has zero width" (mirrors `dp.EPS_KW`).
pub const EPS_KW: f64 = 1e-6;
/// Energy tolerance for "this energy is on that grid point" (mirrors `dp.EPS_KWH`).
pub const EPS_KWH: f64 = 1e-9;

pub(crate) const ENERGY_TOL: f64 = 1e-9;
pub(crate) const VALUE_ABS_TOL: f64 = 1e-9;
pub(crate) const VALUE_REL_TOL: f64 = 1e-12;
pub(crate) const DECISION_ABS_TOL: f64 = 1e-9;
pub(crate) const OBJECTIVE_ABS_TOL: f64 = 1e-6;
pub(crate) const OBJECTIVE_REL_TOL: f64 = 1e-9;

/// A comparison tolerance: an absolute floor plus a relative slack scaled by `scale`.
pub(crate) fn tol(base: f64, rel: f64, scale: f64) -> f64 {
    base + rel * scale.abs()
}

/// The energy tolerance for one scenario (the state grid spans at least one kWh).
pub(crate) fn tol_kwh(config: &crate::config::Config) -> f64 {
    ENERGY_TOL + VALUE_REL_TOL * config.e_max_kwh.abs().max(1.0)
}
