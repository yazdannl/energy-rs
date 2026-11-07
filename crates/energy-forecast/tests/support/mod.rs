//! Shared fixture and model loading for the integration tests.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::assert_is_empty,
    clippy::unreadable_literal
)]

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use energy_forecast::model::PvModel;
use serde_json::Value;

/// The crate directory (fixtures and the artifact live under it).
pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A committed fixture path.
pub fn fixture(name: &str) -> PathBuf {
    crate_dir().join("tests/fixtures").join(name)
}

/// The committed artifact.
pub fn model() -> PvModel {
    PvModel::load_path(&crate_dir().join("models/pv_base.json")).expect("artifact loads")
}

/// The artifact as raw JSON, for corruption tests.
pub fn artifact_json() -> Value {
    read_json(&crate_dir().join("models/pv_base.json"))
}

/// Read one JSON file.
pub fn read_json(path: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(path).expect("fixture is committed");
    serde_json::from_str(&text).expect("fixture is valid JSON")
}

/// Read a JSON-lines file into values.
pub fn json_lines(name: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(fixture(name)).expect("fixture is committed");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("line is valid JSON"))
        .collect()
}

/// Parse one ISO-8601 timestamp as UTC.
pub fn time(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("fixture timestamp is ISO-8601")
        .to_utc()
}

/// The floats of one JSON array.
pub fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_f64().expect("number"))
        .collect()
}

/// The `[timestamp, value]` pairs of one JSON array.
pub fn series(value: &Value) -> Vec<(DateTime<Utc>, f64)> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| {
            let pair = item.as_array().expect("pair");
            (
                time(pair[0].as_str().expect("timestamp")),
                pair[1].as_f64().expect("value"),
            )
        })
        .collect()
}
