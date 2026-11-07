//! Linear decay of the hourly bias factors toward 1.0 over 14 days.

use chrono::{DateTime, Utc};

/// EWMA weight of each new sample.
pub const BIAS_ALPHA: f64 = 0.2;
/// Lower bound of a stored factor.
pub const BIAS_MIN: f64 = 0.3;
/// Upper bound of a stored factor.
pub const BIAS_MAX: f64 = 1.8;
/// Age at which a stored factor has decayed to no correction at all.
pub const BIAS_DECAY_DAYS: f64 = 14.0;
/// Forecasts below this are noise for a measured/forecast ratio and are skipped.
pub const MIN_FORECAST_KW: f64 = 0.05;

/// Decay stored factors toward 1.0 by age, clamped into the served range first.
///
/// A future or absent timestamp is identity; older than the decay window is exactly 1.0.
pub fn decay(
    factors: &[f64; 24],
    updated_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> [f64; 24] {
    let mut out = [1.0_f64; 24];
    for (slot, factor) in out.iter_mut().zip(factors.iter()) {
        *slot = factor.clamp(BIAS_MIN, BIAS_MAX);
    }
    let Some(stamped) = updated_at else {
        return out;
    };
    let age_days = (now - stamped).num_milliseconds() as f64 / 86_400_000.0;
    if age_days <= 0.0 {
        return out;
    }
    if age_days > BIAS_DECAY_DAYS {
        return [1.0_f64; 24];
    }
    let weight = 1.0 - age_days / BIAS_DECAY_DAYS;
    for factor in &mut out {
        *factor = 1.0 + (*factor - 1.0) * weight;
    }
    out
}
