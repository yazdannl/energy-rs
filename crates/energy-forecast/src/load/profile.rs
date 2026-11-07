//! The learned load profile and its persisted payload.
//!
//! The payload has no internal schema version and never had one: readers default every missing key
//! the way `load.py` does, so an old row keeps serving instead of being refused.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::bucket::Bucket;
use super::temperature_fit::TemperatureFit;
use crate::ForecastError;
use crate::stats::{mean, round_digits};

/// A learned 48-bucket profile, or one named device's profile.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadProfile {
    /// `0..24` weekday, `24..48` weekend, in local time.
    pub buckets: [Bucket; 48],
    /// Per-bucket EWMA bias over the last 7 days.
    pub bias: [f64; 48],
    /// Level scale `recent_7d_mean / profile_mean`, clamped to `[0.5, 2.0]`.
    pub scale: f64,
    /// Weighted mean of the weekday and weekend bucket medians.
    pub profile_mean: f64,
    /// Mean load over the last 7 days of the window.
    pub recent_7d_mean: f64,
    /// The temperature regression, if one was fitted.
    pub fit: TemperatureFit,
    /// Hourly samples that met a temperature reading.
    pub temperature_hours: usize,
    /// Hourly samples inside the window.
    pub samples: usize,
    /// Distinct local dates inside the window.
    pub days: usize,
    /// Whether the night tier filled unobserved daytime buckets.
    pub assumed_daytime_shape: bool,
    /// UTC ISO-8601 seconds; when the profile was learned.
    pub updated_at: String,
    /// Device keys subtracted from the house total before this profile was learned.
    pub devices_subtracted: Vec<String>,
    /// Device keys that could not be subtracted, with the reason.
    pub devices_excluded: BTreeMap<String, String>,
    /// House hours left out because a subtracted device had no reading.
    pub devices_dropped_hours: u32,
    /// House hours where the device sum exceeded the total.
    pub devices_clamped_hours: u32,
    /// Device entity, for a device payload.
    pub entity: Option<String>,
    /// Device kind, for a device payload.
    pub kind: Option<String>,
    /// Device display name, for a device payload.
    pub name: Option<String>,
}

impl LoadProfile {
    /// Serialise the payload exactly as the Python store writes it: medians and bias to 4 dp,
    /// unrounded `a`/`b`/`r2`/`t_mean`.
    pub fn to_payload(&self) -> Value {
        let mut map = Map::new();
        self.write_profile(&mut map);
        if self.entity.is_none() {
            self.write_device_columns(&mut map);
        } else {
            self.write_device_identity(&mut map);
        }
        Value::Object(map)
    }

    /// The statistics every payload carries.
    fn write_profile(&self, map: &mut Map<String, Value>) {
        map.insert(
            "buckets".to_string(),
            Value::Array(self.buckets.iter().map(bucket_payload).collect()),
        );
        map.insert(
            "bias".to_string(),
            Value::Array(
                self.bias
                    .iter()
                    .map(|value| number(round_digits(*value, 4)))
                    .collect(),
            ),
        );
        map.insert("scale".to_string(), number(round_digits(self.scale, 4)));
        map.insert(
            "profile_mean".to_string(),
            number(round_digits(self.profile_mean, 4)),
        );
        map.insert(
            "recent_7d_mean".to_string(),
            number(round_digits(self.recent_7d_mean, 4)),
        );
        map.insert("a".to_string(), self.fit.a.map_or(Value::Null, number));
        map.insert("b".to_string(), number(self.fit.b));
        map.insert("r2".to_string(), number(self.fit.r2));
        map.insert(
            "t_mean".to_string(),
            self.fit.t_mean.map_or(Value::Null, number),
        );
        map.insert(
            "temperature_hours".to_string(),
            Value::from(self.temperature_hours as u64),
        );
        map.insert("samples".to_string(), Value::from(self.samples as u64));
        map.insert("days".to_string(), Value::from(self.days as u64));
        map.insert(
            "assumed_daytime_shape".to_string(),
            Value::Bool(self.assumed_daytime_shape),
        );
        map.insert(
            "updated_at".to_string(),
            Value::String(self.updated_at.clone()),
        );
    }

    /// The device accounting a **base** payload carries.
    fn write_device_columns(&self, map: &mut Map<String, Value>) {
        map.insert(
            "devices_subtracted".to_string(),
            Value::Array(
                self.devices_subtracted
                    .iter()
                    .map(|key| Value::String(key.clone()))
                    .collect(),
            ),
        );
        let mut excluded = Map::new();
        for (key, reason) in &self.devices_excluded {
            excluded.insert(key.clone(), Value::String(reason.clone()));
        }
        map.insert("devices_excluded".to_string(), Value::Object(excluded));
        map.insert(
            "devices_dropped_hours".to_string(),
            Value::from(self.devices_dropped_hours),
        );
        map.insert(
            "devices_clamped_hours".to_string(),
            Value::from(self.devices_clamped_hours),
        );
    }

    /// The identity a **device** payload carries instead.
    fn write_device_identity(&self, map: &mut Map<String, Value>) {
        map.insert(
            "entity".to_string(),
            Value::String(self.entity.clone().unwrap_or_default()),
        );
        map.insert(
            "kind".to_string(),
            Value::String(self.kind.clone().unwrap_or_default()),
        );
        map.insert(
            "name".to_string(),
            Value::String(self.name.clone().unwrap_or_default()),
        );
    }

    /// Read a stored payload tolerantly: every missing key defaults, like `load.py` does.
    pub fn from_payload(payload: &Value) -> Result<Self, ForecastError> {
        let map = payload.as_object().ok_or_else(|| {
            ForecastError::Payload("load profile payload is not a JSON object".to_string())
        })?;
        let mut buckets = [Bucket::default(); 48];
        if let Some(Value::Array(items)) = map.get("buckets") {
            for (slot, item) in buckets.iter_mut().zip(items.iter()) {
                *slot = Bucket::from_payload(item);
            }
        }
        let mut bias = [1.0_f64; 48];
        if let Some(Value::Array(items)) = map.get("bias") {
            for (slot, item) in bias.iter_mut().zip(items.iter()) {
                *slot = item.as_f64().unwrap_or(1.0);
            }
        }
        Ok(Self {
            buckets,
            bias,
            scale: float(map, "scale", 1.0),
            profile_mean: float(map, "profile_mean", 0.0),
            recent_7d_mean: float(map, "recent_7d_mean", 0.0),
            fit: TemperatureFit {
                a: map.get("a").and_then(Value::as_f64),
                b: float(map, "b", 0.0),
                r2: float(map, "r2", 0.0),
                t_mean: map.get("t_mean").and_then(Value::as_f64),
            },
            temperature_hours: count(map, "temperature_hours"),
            samples: count(map, "samples"),
            days: count(map, "days"),
            assumed_daytime_shape: map
                .get("assumed_daytime_shape")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            updated_at: map
                .get("updated_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            devices_subtracted: strings(map, "devices_subtracted"),
            devices_excluded: map_of_strings(map, "devices_excluded"),
            devices_dropped_hours: count(map, "devices_dropped_hours") as u32,
            devices_clamped_hours: count(map, "devices_clamped_hours") as u32,
            entity: optional_string(map, "entity"),
            kind: optional_string(map, "kind"),
            name: optional_string(map, "name"),
        })
    }

    /// The median of one bucket, or zero when the payload carried none.
    pub fn median(&self, index: usize) -> f64 {
        self.buckets.get(index).map_or(0.0, |bucket| bucket.median)
    }

    /// Force the temperature term off, as serving does for a kind that forbids one.
    pub fn without_temperature(&mut self) {
        self.fit.b = 0.0;
        self.fit.r2 = 0.0;
    }

    /// The level mean of the weekday buckets, or zero when every one is empty.
    pub fn weekday_mean(&self) -> f64 {
        bucket_mean(&self.buckets[..24])
    }

    /// The level mean of the weekend buckets, or zero when every one is empty.
    pub fn weekend_mean(&self) -> f64 {
        bucket_mean(&self.buckets[24..])
    }
}

fn bucket_mean(buckets: &[Bucket]) -> f64 {
    if !buckets.iter().any(|bucket| bucket.median > 0.0) {
        return 0.0;
    }
    let medians: Vec<f64> = buckets.iter().map(|bucket| bucket.median).collect();
    mean(&medians)
}

fn bucket_payload(bucket: &Bucket) -> Value {
    Value::Object(bucket.to_payload().as_object().cloned().unwrap_or_default())
}

fn number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

fn float(map: &Map<String, Value>, key: &str, fallback: f64) -> f64 {
    map.get(key).and_then(Value::as_f64).unwrap_or(fallback)
}

fn count(map: &Map<String, Value>, key: &str) -> usize {
    map.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(usize::MAX as u64) as usize
}

fn strings(map: &Map<String, Value>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn map_of_strings(map: &Map<String, Value>, key: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(Value::Object(entries)) = map.get(key) {
        for (entry_key, entry_value) in entries {
            if let Some(text) = entry_value.as_str() {
                out.insert(entry_key.clone(), text.to_string());
            }
        }
    }
    out
}

fn optional_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_string)
}
