//! The feasible power windows and the action set the sweep evaluates.

use super::Slot;
use super::{EPS_KW, clamp};
use crate::config::Config;

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
///
/// (`(grid_lo + grid_hi) / 2` and `f64::midpoint` are not the same operation: midpoint halves
/// first and can round differently.  The C core computes the sum, so this must too.)
#[allow(clippy::manual_midpoint)]
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
#[allow(clippy::manual_midpoint)] // the single-step action is `(lo + hi) / 2` in the reference.
pub fn actions(lo: f64, hi: f64, steps: i64, extra: Option<f64>) -> Vec<f64> {
    let steps = steps.max(1);
    if hi - lo <= EPS_KW {
        return vec![clamp(0.0, lo, hi)];
    }
    let mut values = Vec::with_capacity(usize::try_from(steps).unwrap_or(0) + 2);
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
    values.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::with_capacity(values.len());
    for value in values {
        if out.last().is_none_or(|last| (value - last).abs() > 1e-9) {
            out.push(value);
        }
    }
    out
}
