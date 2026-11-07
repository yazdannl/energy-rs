// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here.  The workspace policy (no panics in shipped code) applies to the library.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint
)]

mod support;

use std::sync::Arc;

use energy_ha::{
    Config, Connection, EntityDiffEvent, EntityState, EntityStore, EntityUpdate, watch_entities,
};
use serde_json::{Value, json};
use support::{FakeServer, TOKEN};

#[test]
fn recorded_compressed_frames_apply_to_a_store() {
    let fixture = support::fixture("subscribe_entities.json");
    let frames = fixture["frames"].as_array().expect("frames");
    let mut store = EntityStore::default();
    let mut updates = Vec::new();
    for frame in frames {
        let event: EntityDiffEvent = serde_json::from_value(frame.clone()).expect("event");
        updates.extend(store.apply(&event));
    }
    assert!(!store.is_empty());
    assert!(store.len() >= 3);
    assert!(
        updates
            .iter()
            .any(|update| matches!(update, EntityUpdate::Changed(_)))
    );

    let changed = updates.iter().find_map(|update| match update {
        EntityUpdate::Changed(state) => Some(state),
        _ => None,
    });
    let changed = changed.expect("a changed entity");
    let stored = store.get(&changed.entity_id).expect("stored state");
    assert_eq!(stored, changed);
}

#[test]
fn attribute_changes_merge_and_remove_without_replacing_the_map() {
    let initial = EntityState {
        entity_id: "sensor.flow".to_owned(),
        state: Value::String("1".to_owned()),
        attributes: [
            ("unit_of_measurement".to_owned(), json!("W")),
            ("old".to_owned(), json!(true)),
            ("keep".to_owned(), json!("yes")),
        ]
        .into_iter()
        .collect(),
        last_changed: Some(10.0),
        last_updated: Some(10.0),
    };
    let mut store = EntityStore::from_states([initial]);
    let event: EntityDiffEvent = serde_json::from_value(json!({
        "c": {
            "sensor.flow": {
                "+": {
                    "s": "2",
                    "a": {"unit_of_measurement": "kW", "extra": 3},
                    "lc": 20
                },
                "-": ["old"]
            }
        }
    }))
    .expect("diff");

    let updates = store.apply(&event);
    assert!(matches!(updates[0], EntityUpdate::Changed(_)));
    let state = store.get("sensor.flow").expect("state");
    assert_eq!(state.state, json!("2"));
    assert_eq!(state.unit(), Some("kW"));
    assert_eq!(state.attribute("extra"), Some(&json!(3)));
    assert_eq!(state.attribute("keep"), Some(&json!("yes")));
    assert!(state.attribute("old").is_none());
    assert_eq!(state.last_changed, Some(20.0));
    assert_eq!(state.last_updated, Some(20.0));

    let removal: EntityDiffEvent =
        serde_json::from_value(json!({"r": ["sensor.flow"]})).expect("removal");
    let updates = store.apply(&removal);
    assert_eq!(
        updates,
        vec![EntityUpdate::Removed("sensor.flow".to_owned())]
    );
    assert!(store.is_empty());
}

#[test]
fn null_state_patch_is_present_not_missing() {
    let initial = EntityState {
        entity_id: "sensor.state".to_owned(),
        state: json!("old"),
        ..EntityState::default()
    };
    let mut store = EntityStore::from_states([initial]);
    let event: EntityDiffEvent =
        serde_json::from_value(json!({"c": {"sensor.state": {"+": {"s": null}}}})).expect("diff");
    store.apply(&event);
    assert_eq!(store.get("sensor.state").expect("state").state, Value::Null);
}

#[tokio::test]
async fn compressed_subscription_delivers_snapshot_then_diff() {
    let server = FakeServer::spawn(Arc::new(|_, command, _| match command {
        "subscribe_entities" => support::Reply::ok(Value::Null)
            .with_event(json!({
                "a": {
                    "sensor.flow": {
                        "s": "1",
                        "a": {"unit_of_measurement": "W"},
                        "lc": 100
                    }
                }
            }))
            .with_event(json!({
                "c": {"sensor.flow": {"+": {"s": "2", "lu": 110}}}
            })),
        _ => support::Reply::fail("unknown_command", "unexpected"),
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Connection::connect(&config).await.expect("connect");

    let mut watch = watch_entities(&connection, &["sensor.flow".to_owned()], Vec::new())
        .await
        .expect("watch");
    assert_eq!(watch.snapshot.len(), 1);
    let initial = watch.snapshot.get("sensor.flow").expect("snapshot");
    assert_eq!(initial.state, json!("1"));

    match watch.next().await.expect("update") {
        EntityUpdate::Changed(state) => {
            assert_eq!(state.state, json!("2"));
            assert_eq!(state.last_updated, Some(110.0));
        }
        other => panic!("unexpected update: {other:?}"),
    }
    assert_eq!(
        server.request_types.lock().expect("types")[0],
        "subscribe_entities"
    );
}

#[tokio::test]
async fn falls_back_to_subscribe_events_when_entities_is_unknown() {
    let server = FakeServer::spawn(Arc::new(|_, command, _| match command {
        "subscribe_entities" => support::Reply::fail("unknown_command", "unknown command"),
        "subscribe_events" => support::Reply::ok(Value::Null).with_event(json!({
            "event": {
                "event_type": "state_changed",
                "data": {
                    "entity_id": "sensor.flow",
                    "old_state": {"entity_id": "sensor.flow", "state": "1", "attributes": {}},
                    "new_state": {
                        "entity_id": "sensor.flow",
                        "state": "42",
                        "attributes": {"unit_of_measurement": "W"},
                        "last_changed": "2026-11-07T10:00:00+00:00",
                        "last_updated": "2026-11-07T10:00:05+00:00"
                    }
                }
            }
        })),
        _ => support::Reply::fail("unknown_command", "unexpected"),
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Connection::connect(&config).await.expect("connect");

    let mut watch = watch_entities(&connection, &["sensor.flow".to_owned()], Vec::new())
        .await
        .expect("watch");
    assert!(watch.snapshot.is_empty());

    match watch.next().await.expect("update") {
        EntityUpdate::Changed(state) => {
            assert_eq!(state.numeric(), Some(42.0));
            assert_eq!(state.unit(), Some("W"));
            assert!(state.last_updated.is_some());
        }
        other => panic!("unexpected update: {other:?}"),
    }

    let types = server.request_types.lock().expect("types");
    assert!(types.iter().any(|kind| kind == "subscribe_entities"));
    assert!(types.iter().any(|kind| kind == "subscribe_events"));
}
