//! The learning salvage gate: a schema-versioned payload store that refuses old rows.

// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended API
// here, and fixture numbers read better verbatim than with digit separators.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::assert_is_empty,
    clippy::manual_midpoint,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]

use energy_forecast::ForecastError;
use energy_forecast::persistence::{
    CURRENT_SCHEMA_VERSION, LearningRecord, LearningStore, MemoryLearningStore,
};
use serde_json::json;

fn record(kind: &str, key: &str, version: u32) -> LearningRecord {
    let mut record = LearningRecord::new(
        kind,
        key,
        "2026-03-06T00:00:00+00:00",
        json!({"factors": vec![1.0; 24]}),
    );
    record.schema_version = version;
    record
}

#[test]
fn current_payloads_round_trip_through_the_envelope() {
    let original = record("solar_bias", "hourly", CURRENT_SCHEMA_VERSION);
    let encoded = original.encode();
    let decoded = LearningRecord::decode(&encoded).expect("current schema reads");
    assert_eq!(decoded, original);
}

#[test]
fn an_old_schema_is_refused_not_migrated() {
    let mut encoded = record("solar_bias", "hourly", CURRENT_SCHEMA_VERSION).encode();
    encoded["schema_version"] = json!(0);
    let error = LearningRecord::decode(&encoded).unwrap_err();
    match error {
        ForecastError::SchemaVersion { found, expected } => {
            assert_eq!(found, 0);
            assert_eq!(expected, CURRENT_SCHEMA_VERSION);
        }
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_newer_schema_is_refused_too() {
    let mut encoded = record("solar_bias", "hourly", CURRENT_SCHEMA_VERSION).encode();
    encoded["schema_version"] = json!(CURRENT_SCHEMA_VERSION + 1);
    assert!(matches!(
        LearningRecord::decode(&encoded),
        Err(ForecastError::SchemaVersion { .. })
    ));
}

#[test]
fn a_payload_without_a_version_is_refused() {
    let error = LearningRecord::decode(&json!({"kind": "load", "payload": {}})).unwrap_err();
    assert!(matches!(error, ForecastError::Payload(_)));
    let error = LearningRecord::decode(&json!({"schema_version": 1, "updated_at": "not-a-time"}))
        .unwrap_err();
    assert!(matches!(error, ForecastError::Payload(_)));
    let error = LearningRecord::decode(&json!("nope")).unwrap_err();
    assert!(matches!(error, ForecastError::Payload(_)));
}

#[test]
fn the_memory_store_reads_writes_lists_and_removes() {
    let mut store = MemoryLearningStore::new();
    assert!(store.is_empty());
    store
        .put(record("load", "profile", CURRENT_SCHEMA_VERSION))
        .expect("current schema is accepted");
    store
        .put(record("load", "device:sensor.ev", CURRENT_SCHEMA_VERSION))
        .expect("current schema is accepted");
    store
        .put(record("solar_bias", "hourly", CURRENT_SCHEMA_VERSION))
        .expect("current schema is accepted");
    assert_eq!(store.len(), 3);
    let load = store.get("load", "profile").expect("stored");
    assert_eq!(load.kind, "load");
    let keys: Vec<String> = store.list("load").into_iter().map(|row| row.key).collect();
    assert_eq!(keys, vec!["device:sensor.ev", "profile"], "BTreeMap order");
    assert!(store.remove("load", "profile"));
    assert!(!store.remove("load", "profile"));
    assert!(store.get("load", "profile").is_none());
    assert_eq!(store.len(), 2);
}

#[test]
fn the_memory_store_refuses_an_old_schema_on_write() {
    let mut store = MemoryLearningStore::new();
    let error = store
        .put(record("load", "profile", CURRENT_SCHEMA_VERSION - 1))
        .unwrap_err();
    assert!(matches!(error, ForecastError::SchemaVersion { .. }));
    assert!(store.is_empty());
}
