//! The persisted 24-factor EWMA payload and its update rule.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde_json::{Map, Value};

use super::decay::{BIAS_ALPHA, BIAS_MAX, BIAS_MIN, MIN_FORECAST_KW, decay};
use super::sample::BiasSample;
use crate::ForecastError;
use crate::stats::round_digits;
use crate::time::{iso_seconds, local_hour, parse_iso_utc};

/// The `solar_bias` learning payload: one multiplier per local hour plus its sample counts.
#[derive(Debug, Clone, PartialEq)]
pub struct HourlyBias {
    /// Factors as stored (unclamped values are clamped on read).
    pub factors: [f64; 24],
    /// Samples folded into each hour.
    pub counts: [u32; 24],
    /// Smoothed mean absolute percentage error, if any sample has been scored.
    pub mape_pct: Option<f64>,
    /// When the payload was last written.
    pub updated_at: Option<DateTime<Utc>>,
}

impl Default for HourlyBias {
    fn default() -> Self {
        Self::neutral()
    }
}

impl HourlyBias {
    /// All-ones factors and no samples: no correction in force.
    pub fn neutral() -> Self {
        Self {
            factors: [1.0; 24],
            counts: [0; 24],
            mape_pct: None,
            updated_at: None,
        }
    }

    /// Read a stored payload tolerantly: missing keys default, stored factors are clamped on use.
    pub fn from_payload(payload: &Value) -> Result<Self, ForecastError> {
        let map = payload.as_object().ok_or_else(|| {
            ForecastError::Payload("solar bias payload is not a JSON object".to_string())
        })?;
        let mut bias = Self::neutral();
        if let Some(Value::Array(items)) = map.get("factors") {
            for (slot, item) in bias.factors.iter_mut().zip(items.iter()) {
                *slot = item.as_f64().unwrap_or(1.0).clamp(BIAS_MIN, BIAS_MAX);
            }
        }
        if let Some(Value::Array(items)) = map.get("counts") {
            for (slot, item) in bias.counts.iter_mut().zip(items.iter()) {
                *slot = item.as_u64().unwrap_or(0).min(u64::from(u32::MAX)) as u32;
            }
        }
        bias.mape_pct = map.get("mape_pct").and_then(Value::as_f64);
        bias.updated_at = map
            .get("updated_at")
            .and_then(Value::as_str)
            .and_then(parse_iso_utc);
        Ok(bias)
    }

    /// Serialise the payload exactly as the Python store writes it: factors to 4 dp, MAPE to 2 dp.
    pub fn to_payload(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "factors".to_string(),
            Value::Array(
                self.factors
                    .iter()
                    .map(|factor| json_number(round_digits(*factor, 4)))
                    .collect(),
            ),
        );
        map.insert(
            "counts".to_string(),
            Value::Array(
                self.counts
                    .iter()
                    .map(|count| Value::from(*count))
                    .collect(),
            ),
        );
        map.insert(
            "mape_pct".to_string(),
            match self.mape_pct {
                Some(value) => json_number(round_digits(value.clamp(1.0, 100.0), 2)),
                None => Value::Null,
            },
        );
        map.insert(
            "updated_at".to_string(),
            match self.updated_at {
                Some(moment) => Value::String(iso_seconds(moment)),
                None => Value::Null,
            },
        );
        Value::Object(map)
    }

    /// The factors to apply now: clamped, then decayed by age.
    pub fn factors_at(&self, now: DateTime<Utc>) -> [f64; 24] {
        decay(&self.factors, self.updated_at, now)
    }

    /// One factor, by local hour.
    pub fn factor_at(&self, hour: u32, now: DateTime<Utc>) -> f64 {
        self.factors_at(now)[(hour % 24) as usize]
    }

    /// Fold samples into the EWMA and restamp the payload.
    ///
    /// Forecasts below [`MIN_FORECAST_KW`] and negative measurements are skipped: a multiplier
    /// fitted where almost nothing is produced carries no information.
    pub fn update(&mut self, samples: &[BiasSample], now: DateTime<Utc>, zone: Tz) {
        let mut factors = decay(&self.factors, self.updated_at, now);
        let mut mape = self.mape_pct;
        for sample in samples {
            if !sample.forecast_kw.is_finite() || !sample.measured_kw.is_finite() {
                continue;
            }
            if sample.forecast_kw < MIN_FORECAST_KW || sample.measured_kw < 0.0 {
                continue;
            }
            let hour = local_hour(sample.moment, zone) as usize % 24;
            let ratio = (sample.measured_kw / sample.forecast_kw).clamp(BIAS_MIN, BIAS_MAX);
            factors[hour] = (1.0 - BIAS_ALPHA) * factors[hour] + BIAS_ALPHA * ratio;
            let error_pct =
                (sample.measured_kw - sample.forecast_kw).abs() / sample.forecast_kw * 100.0;
            mape = Some(match mape {
                Some(previous) => (1.0 - BIAS_ALPHA) * previous + BIAS_ALPHA * error_pct,
                None => error_pct,
            });
            self.counts[hour] = self.counts[hour].saturating_add(1);
        }
        self.factors = factors;
        self.mape_pct = mape;
        self.updated_at = Some(now);
    }
}

fn json_number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}
