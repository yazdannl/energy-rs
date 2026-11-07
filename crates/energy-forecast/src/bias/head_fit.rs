//! The closed-form per-hour fit: ridge against a prior of exactly 1.0.

use super::head_sample::HeadSample;

/// Ridge prior strength, in squared kilowatts.
///
/// A bucket needs on the order of this much squared forecast before its own fit outweighs the base
/// model — roughly three hours of a 3 kW array. Deliberately over-strength: a correction fitted
/// from one afternoon stays near the base model, and the promotion gate is the second line of
/// defence.
pub const SHRINKAGE: f64 = 25.0;
/// Lower bound of a fitted factor.
pub const FACTOR_MIN: f64 = 0.3;
/// Upper bound of a fitted factor.
pub const FACTOR_MAX: f64 = 1.8;

/// `m_h = (Σ f·a + λ) / (Σ f² + λ)`, clamped into the served range.
pub fn fit_factors(samples: &[HeadSample], shrinkage: f64) -> [f64; 24] {
    let mut numerator = [shrinkage; 24];
    let mut denominator = [shrinkage; 24];
    for sample in samples {
        if sample.forecast_kw <= 1e-6 || sample.actual_kw < 0.0 {
            continue;
        }
        let bucket = (sample.hour % 24) as usize;
        numerator[bucket] += sample.forecast_kw * sample.actual_kw;
        denominator[bucket] += sample.forecast_kw * sample.forecast_kw;
    }
    let mut factors = [1.0_f64; 24];
    for hour in 0..24 {
        factors[hour] = (numerator[hour] / denominator[hour]).clamp(FACTOR_MIN, FACTOR_MAX);
    }
    factors
}
