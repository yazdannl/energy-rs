//! One entity's state with normalized timestamps.
//!
//! HA emits ISO-8601 over REST/history and epoch seconds in compressed `subscribe_entities`
//! frames; both normalize to epoch seconds (f64).

use std::collections::BTreeMap;

use chrono::DateTime;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// One entity's state, attributes and timestamps.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    #[serde(default)]
    pub entity_id: String,
    #[serde(default)]
    pub state: Value,
    #[serde(default, deserialize_with = "deserialize_attributes")]
    pub attributes: Map<String, Value>,
    #[serde(default, deserialize_with = "deserialize_timestamp")]
    pub last_changed: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_timestamp")]
    pub last_updated: Option<f64>,
}

impl EntityState {
    /// State as a string, when Home Assistant sent one (sensors normally do).
    pub fn state_str(&self) -> Option<&str> {
        self.state.as_str()
    }

    /// Numeric state; `unknown`, `unavailable`, empty and non-numeric values are missing, never 0.
    pub fn numeric(&self) -> Option<f64> {
        match &self.state {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.parse::<f64>().ok(),
            _ => None,
        }
        .filter(|value| value.is_finite())
    }

    /// `unit_of_measurement` from the attributes.
    pub fn unit(&self) -> Option<&str> {
        self.attributes
            .get("unit_of_measurement")
            .and_then(Value::as_str)
    }

    pub fn friendly_name(&self) -> Option<&str> {
        self.attributes.get("friendly_name").and_then(Value::as_str)
    }

    pub fn attribute(&self, key: &str) -> Option<&Value> {
        self.attributes.get(key)
    }

    /// True when the last update is within `max_age_s` of `now_epoch_s`.
    pub fn is_fresh(&self, now_epoch_s: f64, max_age_s: f64) -> bool {
        self.last_updated
            .is_some_and(|updated| now_epoch_s - updated <= max_age_s)
    }
}

/// States indexed by entity id, preserving deterministic iteration order.
pub type EntityMap = BTreeMap<String, EntityState>;

fn deserialize_attributes<'de, D>(deserializer: D) -> Result<Map<String, Value>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Map<String, Value>>::deserialize(deserializer)?.unwrap_or_default())
}

pub(crate) fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => DateTime::parse_from_rfc3339(&text)
            .ok()
            .map(|stamp| stamp.timestamp_millis() as f64 / 1000.0),
        _ => None,
    }))
}
