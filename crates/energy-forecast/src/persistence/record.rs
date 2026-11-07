//! One schema-versioned learning payload.

use serde_json::{Map, Value};

use crate::ForecastError;
use crate::time::parse_iso_utc;

/// The payload schema this build writes and reads.
///
/// Bumped when a payload's shape changes incompatibly. An older row is **refused**, not migrated:
/// silently reinterpreting a payload written by a different arithmetic is how a correction becomes
/// a wrong correction.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// One `learning_profiles` row: kind, key, timestamp and the JSON payload.
#[derive(Debug, Clone, PartialEq)]
pub struct LearningRecord {
    /// Payload schema version.
    pub schema_version: u32,
    /// Profile kind (`solar_bias`, `forecast_head`, `load`, ...).
    pub kind: String,
    /// Profile key (`hourly`, `profile`, `device:<entity>`, ...).
    pub key: String,
    /// Canonical UTC timestamp of the write.
    pub updated_at: String,
    /// The payload itself.
    pub payload: Value,
}

impl LearningRecord {
    /// A record at the current schema version.
    pub fn new(
        kind: impl Into<String>,
        key: impl Into<String>,
        updated_at: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            kind: kind.into(),
            key: key.into(),
            updated_at: updated_at.into(),
            payload,
        }
    }

    /// Parse a stored envelope, refusing a schema this build does not read.
    pub fn decode(value: &Value) -> Result<Self, ForecastError> {
        let map = value.as_object().ok_or_else(|| {
            ForecastError::Payload("learning record is not a JSON object".to_string())
        })?;
        let found = map
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                ForecastError::Payload("learning record has no schema_version".to_string())
            })?;
        let found = u32::try_from(found).unwrap_or(u32::MAX);
        if found != CURRENT_SCHEMA_VERSION {
            return Err(ForecastError::SchemaVersion {
                found,
                expected: CURRENT_SCHEMA_VERSION,
            });
        }
        let updated_at = map
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !updated_at.is_empty() && parse_iso_utc(updated_at).is_none() {
            return Err(ForecastError::Payload(format!(
                "learning record timestamp {updated_at:?} is not ISO-8601"
            )));
        }
        Ok(Self {
            schema_version: found,
            kind: text(map, "kind"),
            key: text(map, "key"),
            updated_at: updated_at.to_string(),
            payload: map.get("payload").cloned().unwrap_or(Value::Null),
        })
    }

    /// Serialise the envelope.
    pub fn encode(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "schema_version".to_string(),
            Value::from(self.schema_version),
        );
        map.insert("kind".to_string(), Value::String(self.kind.clone()));
        map.insert("key".to_string(), Value::String(self.key.clone()));
        map.insert(
            "updated_at".to_string(),
            Value::String(self.updated_at.clone()),
        );
        map.insert("payload".to_string(), self.payload.clone());
        Value::Object(map)
    }
}

fn text(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
