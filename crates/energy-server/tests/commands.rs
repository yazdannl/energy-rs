//! Contract tests for the command routes: validation, persistence and the engine queue.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde_json::json;
use support::{Fixture, patch, post};

use energy_core::dispatch::OverrideAction;
use energy_core::safety::Authority;
use energy_core::stream::EngineCommand;

#[tokio::test]
async fn patching_the_mode_persists_and_notifies_the_engine() {
    let mut fixture = Fixture::new();
    let (status, body) = patch(&fixture.router(), "/api/v1/mode", json!({ "mode": "auto" })).await;
    assert_eq!(status, 200);
    assert_eq!(body["mode"], "auto");
    assert_eq!(body["engineNotified"], true);
    assert!(body["updatedAt"].as_str().unwrap().ends_with('Z'));
    assert_eq!(
        fixture
            .ledger
            .settings()
            .unwrap()
            .get("control.mode")
            .unwrap()
            .as_deref(),
        Some("auto")
    );
    match fixture.handles.commands.try_recv().unwrap() {
        EngineCommand::SetMode(mode) => assert_eq!(mode, Authority::Auto),
        other => panic!("unexpected command {other:?}"),
    }
}

#[tokio::test]
async fn an_unknown_mode_is_422() {
    let fixture = Fixture::new();
    let (status, body) = patch(
        &fixture.router(),
        "/api/v1/mode",
        json!({ "mode": "turbo" }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["details"]["allowed"][0], "advisory");
}

#[tokio::test]
async fn a_mode_change_without_an_engine_is_persisted_but_refused() {
    let fixture = Fixture::new();
    let router = fixture.router();
    drop(fixture.handles);
    let (status, body) = patch(&router, "/api/v1/mode", json!({ "mode": "auto" })).await;
    assert_eq!(status, 503);
    assert_eq!(body["error"]["code"], "engine_unavailable");
    assert_eq!(body["error"]["details"]["persisted"], true);
    assert_eq!(
        fixture
            .ledger
            .settings()
            .unwrap()
            .get("control.mode")
            .unwrap()
            .as_deref(),
        Some("auto")
    );
}

#[tokio::test]
async fn an_override_queues_a_charge_at_the_pack_limit() {
    let mut fixture = Fixture::new();
    let sn = fixture.pack_sn.clone();
    let (status, body) = post(
        &fixture.router(),
        "/api/v1/override",
        json!({ "pack": sn, "action": "charge" }),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(body["accepted"], true);
    assert_eq!(body["action"], "charge");
    assert_eq!(body["powerW"], 800.0);
    assert_eq!(body["ttlS"], 900);
    assert!(body["expiresAt"].as_str().unwrap().ends_with('Z'));
    match fixture.handles.commands.try_recv().unwrap() {
        EngineCommand::Override(command) => {
            assert_eq!(command.action, OverrideAction::Charge { watts: 800.0 });
            assert_eq!(command.ttl_s, 900);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[tokio::test]
async fn a_hold_override_takes_no_power() {
    let mut fixture = Fixture::new();
    let sn = fixture.pack_sn.clone();
    let (status, body) = post(
        &fixture.router(),
        "/api/v1/override",
        json!({ "pack": sn, "action": "hold", "powerW": 500, "ttlS": 60 }),
    )
    .await;
    assert_eq!(status, 202);
    assert!(body["powerW"].is_null());
    assert_eq!(body["ttlS"], 60);
    match fixture.handles.commands.try_recv().unwrap() {
        EngineCommand::Override(command) => assert_eq!(command.action, OverrideAction::Hold),
        other => panic!("unexpected command {other:?}"),
    }
}

#[tokio::test]
async fn an_override_rejects_an_unknown_pack_bad_action_and_bad_numbers() {
    let fixture = Fixture::new();
    let router = fixture.router();

    let (status, body) = post(
        &router,
        "/api/v1/override",
        json!({ "pack": "nope", "action": "charge" }),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "unknown_pack");

    let sn = fixture.pack_sn.clone();
    let (status, body) = post(
        &router,
        "/api/v1/override",
        json!({ "pack": sn, "action": "melt" }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");

    let (status, body) = post(
        &router,
        "/api/v1/override",
        json!({ "pack": sn, "action": "charge", "powerW": 5_000 }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["details"]["maxPowerW"], 800.0);

    let (status, body) = post(
        &router,
        "/api/v1/override",
        json!({ "pack": sn, "action": "hold", "ttlS": 0 }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn protection_queues_a_full_charge() {
    let mut fixture = Fixture::new();
    let sn = fixture.pack_sn.clone();
    let (status, body) = post(
        &fixture.router(),
        "/api/v1/protection",
        json!({ "action": "full_charge", "pack": sn }),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(body["action"], "full_charge");
    assert_eq!(body["pack"], "AB2000-ONE");
    match fixture.handles.commands.try_recv().unwrap() {
        EngineCommand::FullCharge { pack } => assert_eq!(pack.as_deref(), Some("AB2000-ONE")),
        other => panic!("unexpected command {other:?}"),
    }
}

#[tokio::test]
async fn protection_rejects_an_unknown_action_and_pack() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let (status, body) = post(
        &router,
        "/api/v1/protection",
        json!({ "action": "explode" }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");

    let (status, body) = post(
        &router,
        "/api/v1/protection",
        json!({ "action": "full_charge", "pack": "nope" }),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "unknown_pack");
}

#[tokio::test]
async fn learning_queues_a_fleet_relearn() {
    let mut fixture = Fixture::new();
    let (status, body) = post(
        &fixture.router(),
        "/api/v1/learning",
        json!({ "action": "relearn" }),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(body["action"], "relearn");
    assert!(body["pack"].is_null());
    match fixture.handles.commands.try_recv().unwrap() {
        EngineCommand::Relearn { pack } => assert!(pack.is_none()),
        other => panic!("unexpected command {other:?}"),
    }
}
