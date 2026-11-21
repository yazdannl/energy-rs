//! One session as a capacity measurement, with the information it actually carries.

use crate::control::Direction;
use crate::learning::session::SessionRecord;
use crate::learning::weight::recency_weight;

/// Spans this short carry no usable capacity information: a whole-percent BMS can move by one point
/// without the cells moving at all.
pub const MIN_USEFUL_SPAN_PCT: f64 = 5.0;
/// A span this long earns full per-point information.
pub const FULL_SPAN_PCT: f64 = 50.0;
/// Quality-weighted SoC points that bring an estimate to ~63 % confidence.
pub const INFO_SCALE_POINTS: f64 = 400.0;

/// One session's implied capacity, its combination weight and its SoC coverage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapacitySample {
    /// kWh per 100 SoC points the AC energy implies, before any clamping.
    pub capacity_kwh: f64,
    /// Recency times span quality: a short span is barely a measurement.
    pub weight: f64,
    /// SoC points the run actually covered.
    pub soc_points: f64,
}

impl CapacitySample {
    /// Turn one accepted session into a measurement, or `None` when its span is unusable.
    ///
    /// Charging takes `I · x / dSOC` kWh into the cells; discharging delivers `O / x / dSOC`.
    /// Both are divided by the *integer* span the BMS reported, which is the source of the
    /// quantisation this module discounts through [`span_quality`].
    pub fn from_session(session: &SessionRecord, split: f64, rank: usize) -> Option<Self> {
        let span = session.soc_span_pct();
        if !span.is_finite() || span < MIN_USEFUL_SPAN_PCT || !split.is_finite() || split <= 0.0 {
            return None;
        }
        let fraction = span / 100.0;
        let capacity_kwh = match session.direction {
            Direction::Charge => session.energy_ac_kwh * split / fraction,
            Direction::Discharge => session.energy_ac_kwh / split / fraction,
        };
        let weight = recency_weight(rank) * span_quality(span);
        (capacity_kwh.is_finite() && capacity_kwh > 0.0 && weight > 0.0).then_some(Self {
            capacity_kwh,
            weight,
            soc_points: span,
        })
    }
}

/// How much a span of `span_pct` points is worth, in `[0, 1]`.
///
/// One whole-percent endpoint step is 1/span of the reading. Five points is noise, fifty points is
/// a real measurement; everything in between is discounted linearly.
pub fn span_quality(span_pct: f64) -> f64 {
    if !span_pct.is_finite() || span_pct <= MIN_USEFUL_SPAN_PCT {
        return 0.0;
    }
    ((span_pct - MIN_USEFUL_SPAN_PCT) / (FULL_SPAN_PCT - MIN_USEFUL_SPAN_PCT)).clamp(0.0, 1.0)
}

/// The information a span contributes: covered points times their reliability.
pub fn information(span_pct: f64) -> f64 {
    span_pct.max(0.0) * span_quality(span_pct)
}

/// Information to confidence: many short cycles accumulate far slower than a few long ones.
pub fn confidence(samples: &[CapacitySample]) -> f64 {
    let points: f64 = samples
        .iter()
        .map(|sample| information(sample.soc_points))
        .sum();
    1.0 - (-points / INFO_SCALE_POINTS).exp()
}
