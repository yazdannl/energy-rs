//! The SoC and grid windows, computed without the reference's helpers.

use super::arithmetic::clamp;
use super::sample::SlotSample;
use crate::config::Config;

/// The SoC-reachable window for one state.
pub(crate) fn soc_window(config: &Config, energy: f64, dt: f64) -> (f64, f64) {
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
pub(crate) fn grid_window(config: &Config, sample: &SlotSample) -> (f64, f64) {
    let base = sample.base();
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
    lo = export_floor(config, sample, lo);
    if !config.allow_grid_charge {
        let raw = -base;
        let cap = if raw < 0.0 { 0.0 } else { raw };
        if cap < hi {
            hi = cap;
        }
    }
    (sell_floor(config, sample, lo), hi)
}

/// The lower bound the export policy imposes.
fn export_floor(config: &Config, sample: &SlotSample, mut lo: f64) -> f64 {
    let base = sample.base();
    if !config.allow_export {
        if -base > lo {
            lo = -base;
        }
    } else if config.refuse_battery_export {
        let cap = if base > 0.0 { -base } else { 0.0 };
        if cap > lo {
            lo = cap;
        }
    } else if sample.sell <= 0.0 && -base > lo {
        lo = -base;
    }
    lo
}

/// The lower bound the sell-price floor imposes.
fn sell_floor(config: &Config, sample: &SlotSample, mut lo: f64) -> f64 {
    let base = sample.base();
    if let Some(floor) = config.sell_floor_eur_per_kwh {
        if sample.sell < floor {
            let cap = if base > 0.0 { -base } else { 0.0 };
            if cap > lo {
                lo = cap;
            }
        }
        if base > 0.0 && sample.buy < floor && lo < 0.0 {
            lo = 0.0;
        }
    }
    lo
}

/// The intersected feasible window for one state.
// `(grid_lo + grid_hi) / 2` mirrors the reference's fallback, which `f64::midpoint` would change.
#[allow(clippy::manual_midpoint)]
pub(crate) fn feasible(config: &Config, sample: &SlotSample, energy: f64, dt: f64) -> (f64, f64) {
    let (soc_lo, soc_hi) = soc_window(config, energy, dt);
    let (grid_lo, grid_hi) = grid_window(config, sample);
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
