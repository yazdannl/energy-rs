//! One bucket of the learned profile: a median and its p10/p90 band.

use serde_json::{Map, Value};

/// EWMA weight of each new day-bucket ratio.
pub const EWMA_ALPHA: f64 = 0.2;
/// Lower bound of the level scale.
pub const SCALE_MIN: f64 = 0.5;
/// Upper bound of the level scale.
pub const SCALE_MAX: f64 = 2.0;
/// Lower bound of one daily bias ratio.
pub const BIAS_MIN: f64 = 0.5;
/// Upper bound of one daily bias ratio.
pub const BIAS_MAX: f64 = 2.0;
/// Sample count below which the served confidence is `low`.
pub const MIN_SAMPLES: usize = 24;

/// One hour-of-week bucket of the learned profile.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bucket {
    /// Median load, kW.
    pub median: f64,
    /// Tenth percentile, kW.
    pub p10: f64,
    /// Ninetieth percentile, kW.
    pub p90: f64,
    /// Number of hourly samples in the bucket.
    pub count: u32,
}

impl Bucket {
    /// The load at `quantile`, interpolated between the stored p10, p50 and p90.
    ///
    /// A plan that serves the median hour runs out of energy exactly when the house peaks, so the
    /// served level is a quantile above the median (see `LoadInputs::quantile`). The stored
    /// percentiles are the only evidence there is; interpolating between them invents no data.
    pub fn at(&self, quantile: f64) -> f64 {
        let q = quantile.clamp(0.0, 1.0);
        // Exact at the stored percentiles: the golden fixtures compare bits, and `p50 + (p50 - p10)`
        // is not bit-identical to `p50`.
        if q < 0.5 {
            self.p10 + (self.median - self.p10) * (q / 0.5)
        } else if q > 0.5 {
            self.median + (self.p90 - self.median) * ((q - 0.5) / 0.5)
        } else {
            self.median
        }
    }

    /// Read one payload bucket, defaulting every missing statistic to zero.
    pub fn from_payload(value: &Value) -> Self {
        let Some(map) = value.as_object() else {
            return Self::default();
        };
        Self {
            median: number(map, "median"),
            p10: number(map, "p10"),
            p90: number(map, "p90"),
            count: map
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as u32,
        }
    }

    /// Serialise one payload bucket.
    pub fn to_payload(&self) -> Value {
        let mut map = Map::new();
        map.insert("median".to_string(), number_value(self.median));
        map.insert("p10".to_string(), number_value(self.p10));
        map.insert("p90".to_string(), number_value(self.p90));
        map.insert("count".to_string(), Value::from(self.count));
        Value::Object(map)
    }
}

fn number(map: &Map<String, Value>, key: &str) -> f64 {
    map.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn number_value(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}
