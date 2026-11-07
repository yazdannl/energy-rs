//! The per-slot arithmetic the checks re-derive with: energy, wear and grid cost.

use super::Slot;
use crate::config::{Config, PvChargePreference};

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
        if pv_kwh > 0.0 && delta.abs() > 0.0 {
            cost += dt_hours * delta * pv_kwh;
        }
        if config.min_spread_eur_per_kwh > 0.0 {
            cost += config.min_spread_eur_per_kwh * (p_kw - slot.surplus()).max(0.0) * dt_hours;
        }
    }
    cost
}
