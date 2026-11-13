//! The state grid the sweep must have used.

use super::{EPS_KWH, clamp};
use crate::config::Config;

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
    points.sort_by(f64::total_cmp);
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
    let action_count = (config.power_steps + if config.align_actions { 4 } else { 1 }).max(2);
    let slots = i64::try_from(slot_count).unwrap_or(i64::MAX);
    let budget = (config.max_iterations / (slots * action_count).max(1)).max(2);
    if i64::try_from(grid.len()).unwrap_or(i64::MAX) > budget {
        let span = (config.e_max_kwh - config.e_min_kwh).max(0.0);
        let widened = step.max(span / (budget - 1).max(1) as f64);
        (grid, step) = build_grid(config, widened, &anchors);
    }
    (grid, step)
}
