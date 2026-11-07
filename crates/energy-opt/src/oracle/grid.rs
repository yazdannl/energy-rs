//! The state grid: the reference step, its anchors and the complexity coarsening.

use super::arithmetic::clamp;
use crate::config::Config;

/// The grid points and the step actually used.
pub(crate) fn state_grid(config: &Config, slot_count: usize) -> (Vec<f64>, f64) {
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
    let mut grid = build_points(config, step, span, &anchors);
    let action_count = (config.power_steps + 1).max(2);
    let slots = i64::try_from(slot_count).unwrap_or(i64::MAX);
    let budget = (config.max_iterations / (slots * action_count).max(1)).max(2);
    if i64::try_from(grid.len()).unwrap_or(i64::MAX) > budget {
        let widened = step.max(span / (budget - 1).max(1) as f64);
        step = widened;
        grid = build_points(config, step, span, &anchors);
    }
    (grid, step)
}

/// `e_min -> e_max` by `step`, plus the anchors, sorted.
fn build_points(config: &Config, step: f64, span: f64, anchors: &[f64]) -> Vec<f64> {
    let count = ((span / step) as i64).max(0) + 1;
    let mut points: Vec<f64> = (0..count.max(1))
        .map(|i| config.e_min_kwh + i as f64 * step)
        .collect();
    let below_max = points
        .last()
        .is_none_or(|last| *last < config.e_max_kwh - crate::validate::EPS_KWH);
    if below_max {
        points.push(config.e_max_kwh);
    }
    for anchor in anchors {
        let value = clamp(*anchor, config.e_min_kwh, config.e_max_kwh);
        if points.iter().all(|point| (value - point).abs() > 1e-6) {
            points.push(value);
        }
    }
    points.sort_by(f64::total_cmp);
    points
}
