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

use crate::config::{Config, PvChargePreference};
use crate::dp::{SolveInput, SolveResult};
use crate::error::{InputError, ValidationError};

/// Power tolerance for "this window has zero width" (mirrors `dp.EPS_KW`).
pub const EPS_KW: f64 = 1e-6;
/// Energy tolerance for "this energy is on that grid point" (mirrors `dp.EPS_KWH`).
pub const EPS_KWH: f64 = 1e-9;

const ENERGY_TOL: f64 = 1e-9;
const VALUE_ABS_TOL: f64 = 1e-9;
const VALUE_REL_TOL: f64 = 1e-12;
const DECISION_ABS_TOL: f64 = 1e-9;
const OBJECTIVE_ABS_TOL: f64 = 1e-6;
const OBJECTIVE_REL_TOL: f64 = 1e-9;

/// Everything the reference needs from one slot (the `dp.SlotInput` fields the solver reads).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// Forecast PV power, kW.
    pub pv_kw: f64,
    /// Forecast house load, kW.
    pub load_kw: f64,
    /// Import price, EUR/kWh.
    pub buy: f64,
    /// Export price, EUR/kWh; `None` is priced as zero revenue, never as "free export".
    pub sell: f64,
    /// Modelled duration of the slot, minutes.
    pub minutes: f64,
}

impl Slot {
    fn dt(&self) -> f64 {
        self.minutes / 60.0
    }
    fn base(&self) -> f64 {
        self.load_kw - self.pv_kw
    }
    fn surplus(&self) -> f64 {
        (self.pv_kw - self.load_kw).max(0.0)
    }
}

/// `models.clamp`: NaN-free inputs only, so the branch order is the whole behaviour.
pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

/// `dp.next_energy_kwh`: the exact AC-to-stored transition, including at `p == 0`.
pub fn next_energy(config: &Config, e_kwh: f64, p_kw: f64, dt_hours: f64) -> f64 {
    if p_kw >= 0.0 {
        e_kwh + dt_hours * p_kw * config.eta_charge
    } else {
        e_kwh + dt_hours * p_kw / config.eta_discharge
    }
}

/// `dp.wear_cost_eur`: wear per kWh **delivered**, so only the discharge leg pays.
pub fn wear_cost(config: &Config, e_from: f64, e_to: f64) -> f64 {
    config.wear_eur_per_kwh * config.eta_discharge * (e_from - e_to).max(0.0)
}

/// `dp.exact_power_kw`: the power that lands exactly on `e_target` in one slot.
pub fn exact_power(config: &Config, e_kwh: f64, e_target: f64, dt_hours: f64) -> f64 {
    let delta = e_target - e_kwh;
    if delta >= 0.0 {
        if dt_hours > 0.0 {
            delta / (dt_hours * config.eta_charge)
        } else {
            0.0
        }
    } else if dt_hours > 0.0 {
        delta * config.eta_discharge / dt_hours
    } else {
        0.0
    }
}

/// `dp.soc_window_kw`: the SoC-safe `[lo, hi]`; `lo <= hi` always.
pub fn soc_window(config: &Config, e_kwh: f64, dt_hours: f64) -> (f64, f64) {
    let mut lo = -config.p_discharge_kw;
    let mut hi = config.p_charge_kw;
    if dt_hours > 0.0 && config.eta_charge > 0.0 && config.eta_discharge > 0.0 {
        hi = hi.min((config.e_max_kwh - e_kwh).max(0.0) / (dt_hours * config.eta_charge));
        lo = lo.max(-(e_kwh - config.e_min_kwh).max(0.0) * config.eta_discharge / dt_hours);
    }
    (lo.min(hi), hi)
}

/// `dp.grid_window_kw`: the grid-feasible `[lo, hi]` for one slot, from the limits alone.
pub fn grid_window(config: &Config, slot: &Slot) -> (f64, f64) {
    let base = slot.base();
    let mut lo = -config.p_discharge_kw;
    let mut hi = config.p_charge_kw;
    if config.max_import_kw > 0.0 {
        hi = hi.min(config.max_import_kw - base);
    }
    if config.max_export_kw > 0.0 {
        lo = lo.max(-(config.max_export_kw + base));
    }
    if !config.allow_export {
        lo = lo.max(-base);
    } else if config.refuse_battery_export {
        lo = lo.max(-base.max(0.0));
    } else if slot.sell <= 0.0 {
        lo = lo.max(-base);
    }
    if !config.allow_grid_charge {
        hi = hi.min(slot.surplus());
    }
    if let Some(floor) = config.sell_floor_eur_per_kwh {
        if slot.sell < floor {
            lo = lo.max(-base.max(0.0));
        }
        if base > 0.0 && slot.buy < floor {
            lo = lo.max(0.0);
        }
    }
    (lo, hi)
}

/// `dp.power_bounds_kw` without its `grid_binding` flag: the intersected window.
pub fn bounds(config: &Config, slot: &Slot, e_kwh: f64, dt_hours: f64) -> (f64, f64) {
    let (soc_lo, soc_hi) = soc_window(config, e_kwh, dt_hours);
    let (grid_lo, grid_hi) = grid_window(config, slot);
    let mut lo = soc_lo.max(grid_lo);
    let mut hi = soc_hi.min(grid_hi);
    if lo > hi {
        lo = clamp((grid_lo + grid_hi) / 2.0, soc_lo, soc_hi);
        hi = lo;
    }
    if hi < lo {
        lo = clamp(0.0, soc_lo, soc_hi);
        hi = lo;
    }
    (lo, hi)
}

/// `dp._actions`: evenly spaced powers in `[lo, hi]`, `0` always when feasible, sorted and deduped.
pub fn actions(lo: f64, hi: f64, steps: i64, extra: Option<f64>) -> Vec<f64> {
    let steps = steps.max(1);
    if hi - lo <= EPS_KW {
        return vec![clamp(0.0, lo, hi)];
    }
    let mut values = Vec::with_capacity(steps as usize + 2);
    if steps == 1 {
        values.push((lo + hi) / 2.0);
    } else {
        for j in 0..steps {
            values.push(lo + (hi - lo) * j as f64 / (steps - 1) as f64);
        }
    }
    values.push(clamp(0.0, lo, hi));
    if let Some(extra) = extra
        && lo - EPS_KW <= extra
        && extra <= hi + EPS_KW
    {
        values.push(clamp(extra, lo, hi));
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("actions are finite"));
    let mut out: Vec<f64> = Vec::with_capacity(values.len());
    for value in values {
        if out.last().is_none_or(|last| (value - last).abs() > 1e-9) {
            out.push(value);
        }
    }
    out
}

/// `dp._interp`: the piecewise-linear value of `values` at `e_kwh` (bisect_right + linear).
pub fn interp(values: &[f64], grid: &[f64], e_kwh: f64) -> f64 {
    if grid.is_empty() {
        return 0.0;
    }
    if e_kwh <= grid[0] {
        return values[0];
    }
    if e_kwh >= grid[grid.len() - 1] {
        return values[values.len() - 1];
    }
    let mut i = grid.partition_point(|&point| point <= e_kwh);
    i = i.saturating_sub(1);
    if i + 1 >= grid.len() {
        return values[values.len() - 1];
    }
    let span = grid[i + 1] - grid[i];
    if span <= 0.0 {
        return values[i];
    }
    values[i] + (e_kwh - grid[i]) / span * (values[i + 1] - values[i])
}

/// `dp.slot_cost_eur`: the grid cost of one slot at battery power `p_kw` (positive = charging).
pub fn slot_cost(config: &Config, slot: &Slot, dt_hours: f64, p_kw: f64) -> f64 {
    let grid = slot.base() + p_kw;
    let imported = grid.max(0.0);
    let exported = (-grid).max(0.0);
    let mut cost = dt_hours * (slot.buy * imported - slot.sell * exported);
    if p_kw > 0.0 {
        let pv_kwh = p_kw.min(slot.surplus());
        let pv_price = match config.pv_charge_preference {
            PvChargePreference::PreferPv => 0.0,
            PvChargePreference::AvoidPv => slot.buy,
            PvChargePreference::Auto => slot.sell,
        };
        let delta = pv_price - slot.sell;
        if pv_kwh > 0.0 && delta != 0.0 {
            cost += dt_hours * delta * pv_kwh;
        }
        if config.min_spread_eur_per_kwh > 0.0 {
            cost += config.min_spread_eur_per_kwh * (p_kw - slot.surplus()).max(0.0) * dt_hours;
        }
    }
    cost
}

/// The reference one-step cost of `action` at `state`: `(total, slot cost, wear cost)`.
///
/// `total` is what the sweep minimised: the slot's grid cost, the wear of the transition and the
/// interpolated next-row value.  Every operand is checked finite, exactly like `_fast.one_step`.
fn one_step(
    config: &Config,
    slot: &Slot,
    state: f64,
    action: f64,
    dt_hours: f64,
    next_row: &[f64],
    grid: &[f64],
) -> Option<(f64, f64, f64)> {
    let e_next = next_energy(config, state, action, dt_hours);
    if !e_next.is_finite() {
        return None;
    }
    let step = slot_cost(config, slot, dt_hours, action);
    let wear = wear_cost(config, state, e_next);
    let future = interp(next_row, grid, e_next);
    if !(step.is_finite() && wear.is_finite() && future.is_finite()) {
        return None;
    }
    let total = step + wear + future;
    total.is_finite().then_some((total, step, wear))
}

/// The reference strict argmin at `state` over `[lo, hi]`: `(power, cost)`.
///
/// Ties resolve to the lowest power (the action list is sorted and the comparison is strict), so
/// this is the decision the C core stored; `None` means the action set was empty or produced a
/// non-finite cost, which is itself a refusal.
// The parameter list is the reference call spelled out (bounds, the last-slot extra action, the
// next row and the grid); grouping it into a struct would hide which slot value is which.
#[allow(clippy::too_many_arguments)]
pub fn argmin_at(
    config: &Config,
    slot: &Slot,
    state: f64,
    lo: f64,
    hi: f64,
    dt_hours: f64,
    extra: Option<f64>,
    next_row: &[f64],
    grid: &[f64],
) -> Option<(f64, f64)> {
    let mut best_cost = f64::INFINITY;
    let mut best_power: Option<f64> = None;
    for action in actions(lo, hi, config.power_steps, extra) {
        let (total, _, _) = one_step(config, slot, state, action, dt_hours, next_row, grid)?;
        if total < best_cost {
            best_cost = total;
            best_power = Some(action);
        }
    }
    best_power.map(|power| (power, best_cost))
}

/// `dp._build_grid`: `e_min -> e_max` by `step_kwh` plus the anchors, coarsened to the budget.
pub fn build_grid(config: &Config, step_kwh: f64, anchors: &[f64]) -> (Vec<f64>, f64) {
    let span = (config.e_max_kwh - config.e_min_kwh).max(0.0);
    let mut step = step_kwh.max(1e-4);
    let budget = (config.max_states - 3) as f64;
    if span / step > budget - 1.0 {
        step = span / (budget - 1.0);
    }
    let count = ((span / step) as i64 + 1).max(1);
    let mut points: Vec<f64> = (0..count)
        .map(|i| config.e_min_kwh + i as f64 * step)
        .collect();
    if points[points.len() - 1] < config.e_max_kwh - EPS_KWH {
        points.push(config.e_max_kwh);
    }
    for anchor in anchors {
        let value = clamp(*anchor, config.e_min_kwh, config.e_max_kwh);
        if points.iter().all(|point| (value - point).abs() > 1e-6) {
            points.push(value);
        }
    }
    points.sort_by(|a, b| a.partial_cmp(b).expect("grid points are finite"));
    (points, step)
}

/// `_fast._reference_grid`: the grid the C core must have used, including its complexity guard.
pub fn reference_grid(config: &Config, slot_count: usize) -> (Vec<f64>, f64) {
    let anchors: Vec<f64> = match config.e_target {
        Some(target) => vec![
            clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh),
            target,
        ],
        None => vec![clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh)],
    };
    let (mut grid, mut step) = build_grid(config, config.soc_step_kwh, &anchors);
    let action_count = (config.power_steps + 1).max(2);
    let budget = (config.max_iterations / (slot_count as i64 * action_count).max(1)).max(2);
    if grid.len() as i64 > budget {
        let span = (config.e_max_kwh - config.e_min_kwh).max(0.0);
        let widened = step.max(span / (budget - 1).max(1) as f64);
        (grid, step) = build_grid(config, widened, &anchors);
    }
    (grid, step)
}

/// Python's `round()` (half to even) applied to the sampled indices of `_fast._sample_indices`.
fn sample_indices(total: usize, want: usize) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    if total <= want {
        return (0..total).collect();
    }
    let mut out: Vec<usize> = (0..want)
        .map(|i| {
            let raw = i as f64 * (total - 1) as f64 / (want - 1) as f64;
            (raw.round_ties_even() as usize).min(total - 1)
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn tol(base: f64, rel: f64, scale: f64) -> f64 {
    base + rel * scale.abs()
}

/// The per-slot input slice, or a typed error when the arrays disagree or contain `NaN`.
pub(crate) fn slot_at(input: &SolveInput<'_>, index: usize) -> Slot {
    Slot {
        pv_kw: input.pv_kw[index],
        load_kw: input.load_kw[index],
        buy: input.buy[index],
        sell: input.sell[index],
        minutes: input.minutes[index],
    }
}

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

/// Verify a solver result against the reference arithmetic; see the module docs for the list.
///
/// `terminal_row` is the continuation row handed to the solver (`None` for the salvage model).
pub(crate) fn check(
    result: &SolveResult,
    input: &SolveInput<'_>,
    config: &Config,
    terminal_row: Option<&[f64]>,
) -> Result<(), ValidationError> {
    let n = input.len();
    let states = result.states;
    if result.energies.len() != n || result.powers.len() != n || result.windows.len() != n {
        return Err(ValidationError::Length {
            what: "the result path",
            expected: n,
            got: result.energies.len(),
        });
    }
    if result.iterations < 0 || states < 1 || result.grid.len() != states {
        return Err(ValidationError::StateCount {
            states,
            grid: result.grid.len(),
        });
    }
    for (name, value) in [
        ("effective_step_kwh", result.effective_step_kwh),
        ("terminal_error_kwh", result.terminal_error_kwh),
        ("cycle_cost_eur", result.cycle_cost_eur),
        ("objective_eur", result.objective_eur),
        ("internal_objective_eur", result.internal_objective_eur),
        ("start_kwh", config.start_kwh),
        ("salvage", config.salvage),
        ("target_penalty", config.target_penalty),
    ] {
        if !value.is_finite() {
            return Err(ValidationError::NonFiniteScalar(name));
        }
    }
    for (name, series) in [
        ("energies", result.energies.as_slice()),
        ("powers", result.powers.as_slice()),
        ("grid", result.grid.as_slice()),
        ("the value function", result.value.as_slice()),
    ] {
        if series.iter().any(|value| !value.is_finite()) {
            return Err(ValidationError::NonFiniteArray(name));
        }
    }
    if result
        .windows
        .iter()
        .any(|window| !window.values().iter().all(|value| value.is_finite()))
    {
        return Err(ValidationError::NonFiniteArray("windows"));
    }

    // The grid must be exactly the one the reference builds, or every window and interpolation
    // below is on a different discretisation than the sweep used.
    let (grid, reference_step) = reference_grid(config, n);
    if grid.len() != states || result.effective_step_kwh != reference_step {
        return Err(ValidationError::GridMismatch);
    }
    for (got, expected) in result.grid.iter().zip(grid.iter()) {
        if (got - expected).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, *expected) {
            return Err(ValidationError::GridMismatch);
        }
    }

    if result.value.len() != (n + 1) * states {
        return Err(ValidationError::ValueLength);
    }
    let value = &result.value;

    // Terminal row: a supplied row must appear verbatim (it is copied, not recomputed); without
    // one the salvage row is re-derived.  `NaN` fails closed before any tolerance comparison.
    let mut row: Vec<f64> = Vec::new();
    if let Some(supplied) = terminal_row {
        if supplied.len() != states {
            return Err(ValidationError::TerminalRow(
                "the row length does not match the state grid",
            ));
        }
        if supplied.iter().any(|entry| !entry.is_finite()) {
            return Err(ValidationError::TerminalRow("the row is not finite"));
        }
        row.extend_from_slice(supplied);
    }
    for k in 0..states {
        let got = value[n * states + k];
        if terminal_row.is_some() {
            // Copied verbatim by the core, so a correct build reproduces every entry bit for bit.
            if got != row[k] {
                return Err(ValidationError::TerminalRow(
                    "the terminal row does not match the supplied row",
                ));
            }
            continue;
        }
        let mut expected = -config.salvage * grid[k];
        if let Some(target) = config.e_target {
            expected += config.target_penalty * (grid[k] - target).abs();
        }
        if !(got.is_finite() && expected.is_finite()) {
            return Err(ValidationError::TerminalRow(
                "the terminal row is not finite",
            ));
        }
        if (got - expected).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, expected) {
            return Err(ValidationError::TerminalRow(
                "the terminal row does not match the reference",
            ));
        }
    }

    let tol_kwh = ENERGY_TOL + VALUE_REL_TOL * config.e_max_kwh.abs().max(1.0);
    let mut e_now = clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh);
    let mut objective_recomputed = 0.0f64;
    let mut wear_total = 0.0f64;

    for i in 0..n {
        let slot = slot_at(input, i);
        let dt = slot.dt();
        if (result.energies[i] - e_now).abs() > tol_kwh {
            return Err(ValidationError::PathEnergy { slot: i });
        }
        let (soc_lo, soc_hi) = soc_window(config, e_now, dt);
        let (grid_lo, grid_hi) = grid_window(config, &slot);
        let (lo, hi) = bounds(config, &slot, e_now, dt);
        let got = result.windows[i];
        for (value, expected) in [
            (got.soc_lo_kw, soc_lo),
            (got.soc_hi_kw, soc_hi),
            (got.grid_lo_kw, grid_lo),
            (got.grid_hi_kw, grid_hi),
        ] {
            if (value - expected).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, expected) {
                return Err(ValidationError::WindowMismatch { slot: i });
            }
        }
        let power = result.powers[i];
        if power < lo - DECISION_ABS_TOL || power > hi + DECISION_ABS_TOL {
            return Err(ValidationError::Bounds {
                slot: i,
                power,
                lo,
                hi,
            });
        }

        // The stored decision was taken at the grid point the state snaps to, with that point's
        // bounds; the reconstruction clamp below uses this slot's own bounds. Both are reproduced.
        let mut state = e_now;
        let mut action_lo = lo;
        let mut action_hi = hi;
        if let Some(point) = grid
            .iter()
            .find(|point| (**point - e_now).abs() <= ENERGY_TOL)
        {
            state = *point;
            (action_lo, action_hi) = bounds(config, &slot, state, dt);
        }
        let extra = match (i + 1 == n, config.e_target) {
            (true, Some(target)) => Some(exact_power(config, state, target, dt)),
            _ => None,
        };
        let next_row = &value[(i + 1) * states..(i + 2) * states];
        let (best_power, _cost) = argmin_at(
            config, &slot, state, action_lo, action_hi, dt, extra, next_row, &grid,
        )
        .ok_or(ValidationError::EmptyActionSet { slot: i })?;
        let expected_power = clamp(best_power, lo, hi);
        if power != expected_power {
            return Err(ValidationError::Argmin {
                slot: i,
                power,
                expected: expected_power,
            });
        }

        let e_next = next_energy(config, e_now, power, dt);
        if !e_next.is_finite() {
            return Err(ValidationError::PathEnergy { slot: i });
        }
        let step_cost = slot_cost(config, &slot, dt, power);
        let wear = wear_cost(config, e_now, e_next);
        let future = interp(next_row, &grid, e_next);
        if !(step_cost.is_finite() && wear.is_finite() && future.is_finite()) {
            return Err(ValidationError::PathEnergy { slot: i });
        }
        if !(step_cost + wear + future).is_finite() {
            return Err(ValidationError::PathEnergy { slot: i });
        }
        objective_recomputed += step_cost;
        wear_total += wear;
        if !(objective_recomputed.is_finite() && wear_total.is_finite()) {
            return Err(ValidationError::Objective {
                expected: objective_recomputed,
                got: result.objective_eur,
            });
        }
        if e_next < config.e_min_kwh - tol_kwh || e_next > config.e_max_kwh + tol_kwh {
            return Err(ValidationError::PathLeavesBounds { slot: i });
        }
        e_now = clamp(e_next, config.e_min_kwh, config.e_max_kwh);
    }

    if terminal_row.is_some() {
        objective_recomputed += wear_total + interp(&row, &grid, e_now);
    } else {
        objective_recomputed += wear_total - config.salvage * e_now;
    }
    if !objective_recomputed.is_finite() {
        return Err(ValidationError::Objective {
            expected: objective_recomputed,
            got: result.objective_eur,
        });
    }
    if (result.cycle_cost_eur - wear_total).abs()
        > tol(OBJECTIVE_ABS_TOL, OBJECTIVE_REL_TOL, wear_total)
    {
        return Err(ValidationError::CycleCost {
            expected: wear_total,
            got: result.cycle_cost_eur,
        });
    }
    let mut expected_terminal = 0.0;
    if let Some(target) = config.e_target {
        expected_terminal = (e_now - target).abs();
        if terminal_row.is_none() {
            let error = (e_now - target).abs();
            objective_recomputed += config.target_penalty * error;
            if !(objective_recomputed.is_finite() && error.is_finite()) {
                return Err(ValidationError::Objective {
                    expected: objective_recomputed,
                    got: result.objective_eur,
                });
            }
        }
    }
    let tolerance = OBJECTIVE_ABS_TOL + OBJECTIVE_REL_TOL * objective_recomputed.abs();
    if (objective_recomputed - result.objective_eur).abs() > tolerance {
        return Err(ValidationError::Objective {
            expected: objective_recomputed,
            got: result.objective_eur,
        });
    }
    if (expected_terminal - result.terminal_error_kwh).abs() > tolerance {
        return Err(ValidationError::TerminalError {
            expected: expected_terminal,
            got: result.terminal_error_kwh,
        });
    }

    // Sampled backward consistency: V_i[k] must be the reference one-step Bellman value of
    // V_(i+1).  With a terminal row the whole last row is recomputed, because that row is the only
    // place the supplied row binds the sweep; elsewhere five slots and five states are sampled and
    // the periodic full differential bounds the rest.
    for i in sample_indices(n, 5) {
        let slot = slot_at(input, i);
        let dt = slot.dt();
        let next_row = &value[(i + 1) * states..(i + 2) * states];
        let states_to_check = if terminal_row.is_some() && i + 1 == n {
            (0..states).collect()
        } else {
            sample_indices(states, 5)
        };
        for k in states_to_check {
            let e = grid[k];
            let (lo, hi) = bounds(config, &slot, e, dt);
            let extra = match (i + 1 == n, config.e_target) {
                (true, Some(target)) => Some(exact_power(config, e, target, dt)),
                _ => None,
            };
            let (_, best_cost) = argmin_at(config, &slot, e, lo, hi, dt, extra, next_row, &grid)
                .ok_or(ValidationError::EmptyActionSet { slot: i })?;
            let got = value[i * states + k];
            if !(got.is_finite() && best_cost.is_finite()) {
                return Err(ValidationError::Backward { slot: i, state: k });
            }
            if (got - best_cost).abs() > tol(VALUE_ABS_TOL, VALUE_REL_TOL, best_cost) {
                return Err(ValidationError::Backward { slot: i, state: k });
            }
        }
    }
    Ok(())
}
