//! The naive per-slot arithmetic: deliberately a second implementation.

use super::sample::SlotSample;
use crate::config::{Config, PvChargePreference};

/// `models.clamp`, written out.
pub(crate) fn clamp(value: f64, low: f64, high: f64) -> f64 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

/// The exact energy transition of one action.
pub(crate) fn transition(config: &Config, energy: f64, power: f64, dt: f64) -> f64 {
    if power >= 0.0 {
        energy + dt * power * config.eta_charge
    } else {
        energy + dt * power / config.eta_discharge
    }
}

/// The wear of one transition (discharge leg only).
pub(crate) fn wear(config: &Config, from: f64, to: f64) -> f64 {
    config.wear_eur_per_kwh * config.eta_discharge * (from - to).max(0.0)
}

/// The power that lands exactly on the target in one slot.
pub(crate) fn exact_power(config: &Config, energy: f64, target: f64, dt: f64) -> f64 {
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

/// Linear interpolation of a value row at an exact energy (the naive `bisect_right`).
pub(crate) fn interp(values: &[f64], grid: &[f64], energy: f64) -> f64 {
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

/// Evenly spaced actions in `[lo, hi]`, `0` always included when feasible, sorted and deduped.
// `(lo + hi) / 2` is the reference's single-step action; `f64::midpoint` rounds differently.
#[allow(clippy::manual_midpoint)]
pub(crate) fn actions(lo: f64, hi: f64, steps: i64, extra: Option<f64>) -> Vec<f64> {
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
    values.sort_by(f64::total_cmp);
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

/// The grid cost of one slot at one power.
pub(crate) fn slot_cost(config: &Config, sample: &SlotSample, power: f64) -> f64 {
    let grid = sample.base() + power;
    let mut cost = sample.dt() * (sample.buy * grid.max(0.0) - sample.sell * (-grid).max(0.0));
    if power > 0.0 {
        let surplus = sample.surplus();
        let pv_kwh = if power < surplus { power } else { surplus };
        let pv_price = match config.pv_charge_preference {
            PvChargePreference::PreferPv => 0.0,
            PvChargePreference::AvoidPv => sample.buy,
            PvChargePreference::Auto => sample.sell,
        };
        if pv_kwh > 0.0 && (pv_price - sample.sell).abs() > 0.0 {
            cost += sample.dt() * (pv_price - sample.sell) * pv_kwh;
        }
        if config.min_spread_eur_per_kwh > 0.0 {
            cost += config.min_spread_eur_per_kwh * (power - surplus).max(0.0) * sample.dt();
        }
    }
    cost
}
