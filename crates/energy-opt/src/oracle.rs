//! Brute-force reference DP for the differential checks (`PLAN.md` §6).
//!
//! Deliberately slow and self-contained: it builds its own state grid, sweeps every state with a
//! fresh action list, and reconstructs the path in the most direct way possible, so that it shares
//! no arithmetic with the C core and no helper with [`crate::validate`].  A disagreement between
//! the two is therefore a real disagreement, not a shared typo.
//!
//! Used by the differential tests and, in the engine, by the periodic full differential; never on
//! the fast path (a solve here costs a few milliseconds, which the 15-minute replan does not need
//! on every tick).

use crate::config::{Config, PvChargePreference};
use crate::dp::SolveInput;
use crate::error::SolveError;

/// One brute-force solve; the fields line up with [`crate::dp::SolveResult`].
#[derive(Debug, Clone, PartialEq)]
pub struct OracleResult {
    pub grid: Vec<f64>,
    pub value: Vec<f64>,
    /// The stored choice per `(slot, state)`, `slots * states` row-major.
    pub argmin: Vec<f64>,
    pub energies: Vec<f64>,
    pub powers: Vec<f64>,
    /// SoC and grid windows per slot: `[soc_lo, soc_hi, grid_lo, grid_hi]`.
    pub windows: Vec<[f64; 4]>,
    pub objective_eur: f64,
    pub cycle_cost_eur: f64,
    pub terminal_error_kwh: f64,
    pub effective_step_kwh: f64,
    pub internal_objective_eur: f64,
    pub iterations: i64,
    pub states: usize,
}

fn clamp(value: f64, low: f64, high: f64) -> f64 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

/// The SoC-reachable window for one state.
fn soc_window(config: &Config, energy: f64, dt: f64) -> (f64, f64) {
    let mut lo = -config.p_discharge_kw;
    let mut hi = config.p_charge_kw;
    if dt > 0.0 && config.eta_charge > 0.0 && config.eta_discharge > 0.0 {
        let room = (config.e_max_kwh - energy).max(0.0);
        let reachable = room / (dt * config.eta_charge);
        if reachable < hi {
            hi = reachable;
        }
        let above = (energy - config.e_min_kwh).max(0.0);
        let reachable = -above * config.eta_discharge / dt;
        if reachable > lo {
            lo = reachable;
        }
    }
    if lo > hi {
        lo = hi;
    }
    (lo, hi)
}

/// The grid-feasible window for one slot.
fn grid_window(config: &Config, pv_kw: f64, load_kw: f64, buy: f64, sell: f64) -> (f64, f64) {
    let base = load_kw - pv_kw;
    let mut lo = -config.p_discharge_kw;
    let mut hi = config.p_charge_kw;
    if config.max_import_kw > 0.0 {
        let cap = config.max_import_kw - base;
        if cap < hi {
            hi = cap;
        }
    }
    if config.max_export_kw > 0.0 {
        let cap = -(config.max_export_kw + base);
        if cap > lo {
            lo = cap;
        }
    }
    if !config.allow_export {
        if -base > lo {
            lo = -base;
        }
    } else if config.refuse_battery_export {
        let cap = if base > 0.0 { -base } else { 0.0 };
        if cap > lo {
            lo = cap;
        }
    } else if sell <= 0.0 && -base > lo {
        lo = -base;
    }
    if !config.allow_grid_charge {
        let cap = -(base);
        let cap = if cap < 0.0 { 0.0 } else { cap };
        if cap < hi {
            hi = cap;
        }
    }
    if let Some(floor) = config.sell_floor_eur_per_kwh {
        if sell < floor {
            let cap = if base > 0.0 { -base } else { 0.0 };
            if cap > lo {
                lo = cap;
            }
        }
        if base > 0.0 && buy < floor && lo < 0.0 {
            lo = 0.0;
        }
    }
    (lo, hi)
}

/// The intersected feasible window for one state.
fn feasible(
    config: &Config,
    pv_kw: f64,
    load_kw: f64,
    buy: f64,
    sell: f64,
    energy: f64,
    dt: f64,
) -> (f64, f64) {
    let (soc_lo, soc_hi) = soc_window(config, energy, dt);
    let (grid_lo, grid_hi) = grid_window(config, pv_kw, load_kw, buy, sell);
    let mut lo = if soc_lo > grid_lo { soc_lo } else { grid_lo };
    let mut hi = if soc_hi < grid_hi { soc_hi } else { grid_hi };
    if lo > hi {
        let middle = (grid_lo + grid_hi) / 2.0;
        lo = clamp(middle, soc_lo, soc_hi);
        hi = lo;
    }
    if hi < lo {
        lo = clamp(0.0, soc_lo, soc_hi);
        hi = lo;
    }
    (lo, hi)
}

/// Evenly spaced actions in `[lo, hi]`, `0` always included when feasible, sorted and deduped.
fn actions(lo: f64, hi: f64, steps: i64, extra: Option<f64>) -> Vec<f64> {
    let steps = steps.max(1);
    if hi - lo <= crate::validate::EPS_KW {
        return vec![clamp(0.0, lo, hi)];
    }
    let mut values = Vec::new();
    if steps == 1 {
        values.push((lo + hi) / 2.0);
    } else {
        for j in 0..steps {
            values.push(lo + (hi - lo) * j as f64 / (steps - 1) as f64);
        }
    }
    values.push(clamp(0.0, lo, hi));
    if let Some(extra) = extra
        && lo - crate::validate::EPS_KW <= extra
        && extra <= hi + crate::validate::EPS_KW
    {
        values.push(clamp(extra, lo, hi));
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("finite actions"));
    let mut out: Vec<f64> = Vec::new();
    for value in values {
        let duplicate = match out.last() {
            Some(last) => (value - last).abs() <= 1e-9,
            None => false,
        };
        if !duplicate {
            out.push(value);
        }
    }
    out
}

/// Linear interpolation of a value row at an exact energy (the naive `bisect_right`).
fn interp(values: &[f64], grid: &[f64], energy: f64) -> f64 {
    if grid.is_empty() {
        return 0.0;
    }
    let mut right = 0usize;
    while right < grid.len() && grid[right] <= energy {
        right += 1;
    }
    if right == 0 {
        return values[0];
    }
    if right >= grid.len() {
        return values[values.len() - 1];
    }
    let i = right - 1;
    let span = grid[i + 1] - grid[i];
    if span <= 0.0 {
        return values[i];
    }
    values[i] + (energy - grid[i]) / span * (values[i + 1] - values[i])
}

/// The exact energy transition of one action.
fn transition(config: &Config, energy: f64, power: f64, dt: f64) -> f64 {
    if power >= 0.0 {
        energy + dt * power * config.eta_charge
    } else {
        energy + dt * power / config.eta_discharge
    }
}

/// The grid cost of one slot at one power.
fn slot_cost(
    config: &Config,
    pv_kw: f64,
    load_kw: f64,
    buy: f64,
    sell: f64,
    power: f64,
    dt: f64,
) -> f64 {
    let base = load_kw - pv_kw;
    let grid = base + power;
    let mut cost = dt * (buy * grid.max(0.0) - sell * (-grid).max(0.0));
    if power > 0.0 {
        let surplus = (pv_kw - load_kw).max(0.0);
        let pv_kwh = if power < surplus { power } else { surplus };
        let pv_price = match config.pv_charge_preference {
            PvChargePreference::PreferPv => 0.0,
            PvChargePreference::AvoidPv => buy,
            PvChargePreference::Auto => sell,
        };
        if pv_kwh > 0.0 && pv_price != sell {
            cost += dt * (pv_price - sell) * pv_kwh;
        }
        if config.min_spread_eur_per_kwh > 0.0 {
            cost += config.min_spread_eur_per_kwh * (power - surplus).max(0.0) * dt;
        }
    }
    cost
}

/// The wear of one transition (discharge leg only).
fn wear(config: &Config, from: f64, to: f64) -> f64 {
    config.wear_eur_per_kwh * config.eta_discharge * (from - to).max(0.0)
}

/// The power that lands exactly on the target in one slot.
fn exact_power(config: &Config, energy: f64, target: f64, dt: f64) -> f64 {
    let delta = target - energy;
    if delta >= 0.0 {
        if dt > 0.0 {
            delta / (dt * config.eta_charge)
        } else {
            0.0
        }
    } else if dt > 0.0 {
        delta * config.eta_discharge / dt
    } else {
        0.0
    }
}

/// The grid points: the reference step, its anchors and the complexity coarsening.
fn state_grid(config: &Config, slot_count: usize) -> (Vec<f64>, f64) {
    let span = (config.e_max_kwh - config.e_min_kwh).max(0.0);
    let mut step = config.soc_step_kwh.max(1e-4);
    let budget = (config.max_states - 3) as f64;
    if span / step > budget - 1.0 {
        step = span / (budget - 1.0);
    }
    let anchors: Vec<f64> = match config.e_target {
        Some(target) => vec![
            clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh),
            target,
        ],
        None => vec![clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh)],
    };
    let build = |step: f64| {
        let count = ((span / step) as i64).max(0) + 1;
        let mut points: Vec<f64> = (0..count.max(1))
            .map(|i| config.e_min_kwh + i as f64 * step)
            .collect();
        if *points.last().expect("at least one point") < config.e_max_kwh - crate::validate::EPS_KWH
        {
            points.push(config.e_max_kwh);
        }
        for anchor in &anchors {
            let value = clamp(*anchor, config.e_min_kwh, config.e_max_kwh);
            if points.iter().all(|point| (value - point).abs() > 1e-6) {
                points.push(value);
            }
        }
        points.sort_by(|a, b| a.partial_cmp(b).expect("finite grid"));
        points
    };
    let mut grid = build(step);
    let action_count = (config.power_steps + 1).max(2);
    let budget = (config.max_iterations / (slot_count as i64 * action_count).max(1)).max(2);
    if grid.len() as i64 > budget {
        let widened = step.max(span / (budget - 1).max(1) as f64);
        step = widened;
        grid = build(step);
    }
    (grid, step)
}

/// Run the reference DP over `input`.
pub fn solve(
    config: &Config,
    input: &SolveInput<'_>,
    terminal_row: Option<&[f64]>,
) -> Result<OracleResult, SolveError> {
    config.validate()?;
    crate::validate::check_input(input)?;
    let n = input.len();
    let (grid, step) = state_grid(config, n);
    let m = grid.len();
    if let Some(row) = terminal_row
        && row.len() != m
    {
        return Err(SolveError::TerminalRow(format!(
            "the row has {} entries for a {m}-state grid",
            row.len()
        )));
    }

    let mut value = vec![0.0f64; (n + 1) * m];
    let mut argmin = vec![0.0f64; n * m];
    for k in 0..m {
        value[n * m + k] = match terminal_row {
            Some(row) => row[k],
            None => {
                let mut entry = -config.salvage * grid[k];
                if let Some(target) = config.e_target {
                    entry += config.target_penalty * (grid[k] - target).abs();
                }
                entry
            }
        };
    }

    let mut iterations = 0i64;
    for i in (0..n).rev() {
        let dt = input.minutes[i] / 60.0;
        for k in 0..m {
            let energy = grid[k];
            let (lo, hi) = feasible(
                config,
                input.pv_kw[i],
                input.load_kw[i],
                input.buy[i],
                input.sell[i],
                energy,
                dt,
            );
            let extra = match (i + 1 == n, config.e_target) {
                (true, Some(target)) => Some(exact_power(config, energy, target, dt)),
                _ => None,
            };
            let mut best = f64::INFINITY;
            let mut choice = clamp(0.0, lo, hi);
            for power in actions(lo, hi, config.power_steps, extra) {
                let next = transition(config, energy, power, dt);
                let cost = slot_cost(
                    config,
                    input.pv_kw[i],
                    input.load_kw[i],
                    input.buy[i],
                    input.sell[i],
                    power,
                    dt,
                ) + wear(config, energy, next)
                    + interp(&value[(i + 1) * m..(i + 2) * m], &grid, next);
                iterations += 1;
                if cost < best {
                    best = cost;
                    choice = power;
                }
            }
            value[i * m + k] = best;
            argmin[i * m + k] = choice;
        }
    }

    let start = clamp(config.start_kwh, config.e_min_kwh, config.e_max_kwh);
    let mut start_index = 0usize;
    for k in 1..m {
        if (grid[k] - start).abs() < (grid[start_index] - start).abs() {
            start_index = k;
        }
    }

    let mut energies = Vec::with_capacity(n);
    let mut powers = Vec::with_capacity(n);
    let mut windows = Vec::with_capacity(n);
    let mut cycle_cost = 0.0f64;
    let mut objective = 0.0f64;
    let mut energy = start;
    for i in 0..n {
        let dt = input.minutes[i] / 60.0;
        let (lo, hi) = feasible(
            config,
            input.pv_kw[i],
            input.load_kw[i],
            input.buy[i],
            input.sell[i],
            energy,
            dt,
        );
        let (soc_lo, soc_hi) = soc_window(config, energy, dt);
        let (grid_lo, grid_hi) = grid_window(
            config,
            input.pv_kw[i],
            input.load_kw[i],
            input.buy[i],
            input.sell[i],
        );
        windows.push([soc_lo, soc_hi, grid_lo, grid_hi]);
        let index = (0..m).find(|k| (grid[*k] - energy).abs() <= 1e-9);
        let chosen = match index {
            Some(k) => argmin[i * m + k],
            None => {
                let extra = match (i + 1 == n, config.e_target) {
                    (true, Some(target)) => Some(exact_power(config, energy, target, dt)),
                    _ => None,
                };
                let mut best = f64::INFINITY;
                let mut choice = clamp(0.0, lo, hi);
                for power in actions(lo, hi, config.power_steps, extra) {
                    let next = transition(config, energy, power, dt);
                    let cost = slot_cost(
                        config,
                        input.pv_kw[i],
                        input.load_kw[i],
                        input.buy[i],
                        input.sell[i],
                        power,
                        dt,
                    ) + wear(config, energy, next)
                        + interp(&value[(i + 1) * m..(i + 2) * m], &grid, next);
                    if cost < best {
                        best = cost;
                        choice = power;
                    }
                }
                choice
            }
        };
        let power = clamp(chosen, lo, hi);
        let next = transition(config, energy, power, dt);
        energies.push(energy);
        powers.push(power);
        cycle_cost += wear(config, energy, next);
        objective += slot_cost(
            config,
            input.pv_kw[i],
            input.load_kw[i],
            input.buy[i],
            input.sell[i],
            power,
            dt,
        );
        energy = clamp(next, config.e_min_kwh, config.e_max_kwh);
    }
    if let Some(row) = terminal_row {
        objective += cycle_cost + interp(row, &grid, energy);
    } else {
        objective += cycle_cost - config.salvage * energy;
        if let Some(target) = config.e_target {
            objective += config.target_penalty * (energy - target).abs();
        }
    }
    let terminal_error = match config.e_target {
        Some(target) => (energy - target).abs(),
        None => 0.0,
    };

    let internal = value[start_index];
    Ok(OracleResult {
        grid,
        value,
        argmin,
        energies,
        powers,
        windows,
        objective_eur: objective,
        cycle_cost_eur: cycle_cost,
        terminal_error_kwh: terminal_error,
        effective_step_kwh: step,
        internal_objective_eur: internal,
        iterations,
        states: m,
    })
}
