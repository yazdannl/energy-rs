//! Tolerant report parsing.

use serde_json::{Map, Value};

use crate::error::BatteryError;
use crate::report::decode;
use crate::report::device_report::Report;
use crate::report::pack::PackReport;
use crate::report::properties::parse_properties;

/// Parse a report body, accepting both field envelopes.
///
/// The property map is built as "top-level scalars, then `properties` on top", so a firmware that
/// keeps a value at the top level and one that nests it both read; `packData` is looked up in the
/// same merged map, which covers packs nested under `properties`. Unknown keys are ignored.
pub fn parse_report(body: &[u8]) -> Result<Report, BatteryError> {
    let root: Value = serde_json::from_slice(body).map_err(|err| BatteryError::Payload {
        reason: format!("report is not JSON: {err}"),
    })?;
    let object = root.as_object().ok_or_else(|| BatteryError::Payload {
        reason: "report is not a JSON object".to_owned(),
    })?;
    let sn = decode::text(object, "sn").ok_or_else(|| BatteryError::Payload {
        reason: "report carries no sn".to_owned(),
    })?;
    let merged = merge_envelope(object);
    Ok(Report {
        timestamp: decode::int(&merged, "timestamp")
            .or_else(|| decode::int(&merged, "ts"))
            .unwrap_or_default(),
        message_id: decode::int(&merged, "messageId")
            .map_or(0, |raw| u32::try_from(raw).unwrap_or(0)),
        version: decode::int(&merged, "version").and_then(|raw| u32::try_from(raw).ok()),
        product: decode::text(&merged, "product"),
        properties: parse_properties(&merged),
        packs: parse_packs(&merged),
        sn,
    })
}

fn merge_envelope(object: &Map<String, Value>) -> Map<String, Value> {
    let mut merged = object.clone();
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (key, value) in properties {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

fn parse_packs(merged: &Map<String, Value>) -> Vec<PackReport> {
    merged
        .get("packData")
        .and_then(Value::as_array)
        .map(|packs| packs.iter().filter_map(PackReport::from_value).collect())
        .unwrap_or_default()
}
