//! The default residential shape used by the fallback tier and the night fill.

/// Generic residential shape: 24 relative hourly weights, normalised where it is used.
pub const DEFAULT_SHAPE: [f64; 24] = [
    0.75, 0.65, 0.60, 0.60, 0.65, 0.85, 1.15, 1.35, 1.25, 1.05, 0.95, 0.95, 0.95, 0.95, 0.95, 1.05,
    1.30, 1.55, 1.55, 1.35, 1.15, 1.00, 0.90, 0.80,
];

/// The p10 band used when a bucket has no quantiles of its own.
pub const FALLBACK_P10_RATIO: f64 = 0.7;
/// The p90 band used when a bucket has no quantiles of its own.
pub const FALLBACK_P90_RATIO: f64 = 1.3;

/// Replace buckets with no samples by the default shape, at the observed level.
///
/// Only meaningful for a profile learned from night hours, where an empty bucket is a fact about
/// the observation window rather than about the house. The factor is chosen so the finished day
/// averages exactly the observed level (the mean of the **observed** buckets), not the shape's own
/// mean: normalising by the shape mean makes a synthetic 1 kW house come out 5.7% high.
pub fn fill_assumed_shape(medians: &mut [f64; 48], p10s: &mut [f64; 48], p90s: &mut [f64; 48]) {
    if !medians.iter().any(|value| *value > 0.0) {
        return;
    }
    let observed_values: Vec<f64> = medians
        .iter()
        .copied()
        .filter(|value| *value > 0.0)
        .collect();
    let observed = crate::stats::mean(&observed_values);
    let empty: Vec<usize> = medians
        .iter()
        .enumerate()
        .filter(|(_, value)| **value <= 0.0)
        .map(|(index, _)| index)
        .collect();
    if empty.is_empty() {
        return;
    }
    let shape_total: f64 = empty.iter().map(|index| DEFAULT_SHAPE[index % 24]).sum();
    let factor = if shape_total > 0.0 {
        observed * empty.len() as f64 / shape_total
    } else {
        0.0
    };
    for index in empty {
        let value = DEFAULT_SHAPE[index % 24] * factor;
        medians[index] = crate::stats::round_digits(value, 4);
        if p10s[index] <= 0.0 {
            p10s[index] = crate::stats::round_digits(value * 0.55, 4);
        }
        if p90s[index] <= 0.0 {
            p90s[index] = crate::stats::round_digits(value * 1.55, 4);
        }
    }
}
