//! Contract tests for `GET`/`PATCH /api/v1/settings`: schema, typed values, validation and the
//! timestamped (as-of) write rules.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;
use support::{Fixture, get, patch};

fn stamp(epoch: i64) -> String {
    DateTime::from_timestamp(epoch, 0)
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[tokio::test]
async fn settings_render_typed_values_and_the_schema() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/settings").await;
    assert_eq!(status, 200);
    assert_eq!(body["values"]["reserve_soc_pct"], 20.0);
    assert_eq!(body["values"]["grid_import_limit_w"], 0.0);
    assert_eq!(body["values"]["full_charge_weekday"], "sun");
    let schema = body["schema"].as_array().unwrap();
    assert_eq!(schema.len(), 18);
    assert_eq!(schema[0]["key"], "grid_import_limit_w");
    assert_eq!(schema[0]["kind"], "number");
    assert_eq!(schema[0]["min"], 0.0);
    assert_eq!(schema[0]["max"], 20_000.0);
    assert_eq!(schema[0]["default"], 0.0);
}

#[tokio::test]
async fn settings_as_of_reads_history_not_the_present() {
    let fixture = Fixture::new();
    let uri = format!("/api/v1/settings?asOf={}", stamp(fixture.base - 1_000));
    let (status, body) = get(&fixture.router(), &uri).await;
    assert_eq!(status, 200);
    assert_eq!(body["values"]["reserve_soc_pct"], 10.0);
    assert_eq!(body["asOf"], stamp(fixture.base - 1_000));
}

#[tokio::test]
async fn patching_a_setting_writes_the_ledger_and_returns_the_typed_value() {
    let fixture = Fixture::new();
    let (status, body) = patch(
        &fixture.router(),
        "/api/v1/settings",
        json!({ "values": { "grid_import_limit_w": 2_000 } }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["values"]["grid_import_limit_w"], 2_000.0);
    // The response carries the whole state so the form can replace its draft with it.
    assert_eq!(body["values"].as_object().unwrap().len(), 18);
    assert_eq!(body["values"]["reserve_soc_pct"], 20.0);
    assert_eq!(
        fixture
            .ledger
            .settings()
            .unwrap()
            .get("grid_import_limit_w")
            .unwrap()
            .as_deref(),
        Some("2000")
    );
}

#[tokio::test]
async fn patching_a_setting_rejects_bad_values_and_unknown_keys() {
    let fixture = Fixture::new();
    let router = fixture.router();

    let (status, body) = patch(
        &router,
        "/api/v1/settings",
        json!({ "values": { "reserve_soc_pct": 99 } }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "invalid_setting");
    assert_eq!(body["error"]["details"]["key"], "reserve_soc_pct");

    let (status, body) = patch(
        &router,
        "/api/v1/settings",
        json!({ "values": { "zonneplan_refresh_token": "x" } }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "unsupported_setting");

    let (status, body) = patch(
        &router,
        "/api/v1/settings",
        json!({ "values": { "mode": "auto" } }),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "unsupported_setting");
    assert!(body["error"]["details"]["hint"].is_string());

    let (status, body) = patch(&router, "/api/v1/settings", json!({ "values": {} })).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn a_timestamped_write_cannot_rewrite_history() {
    let fixture = Fixture::new();
    let uri = "/api/v1/settings";
    let body = json!({
        "values": { "reserve_soc_pct": 30 },
        "at": stamp(fixture.base - 1_000),
    });
    let (status, response) = patch(&fixture.router(), uri, body).await;
    assert_eq!(status, 409);
    assert_eq!(response["error"]["code"], "stale_write");
    assert_eq!(response["error"]["details"]["key"], "reserve_soc_pct");
    assert_eq!(
        fixture
            .ledger
            .settings()
            .unwrap()
            .get("reserve_soc_pct")
            .unwrap()
            .as_deref(),
        Some("20")
    );
}

#[tokio::test]
async fn a_future_write_timestamp_is_422() {
    let fixture = Fixture::new();
    let future = Utc::now().timestamp() + 3_600;
    let body = json!({
        "values": { "replan_seconds": 1200 },
        "at": stamp(future),
    });
    let (status, response) = patch(&fixture.router(), "/api/v1/settings", body).await;
    assert_eq!(status, 422);
    assert_eq!(response["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn a_timestamped_write_at_the_present_succeeds_and_keeps_history() {
    let fixture = Fixture::new();
    let at = fixture.base + 60;
    let body = json!({
        "values": { "reserve_soc_pct": 25 },
        "at": stamp(at),
    });
    let (status, response) = patch(&fixture.router(), "/api/v1/settings", body).await;
    assert_eq!(status, 200);
    assert_eq!(response["values"]["reserve_soc_pct"], 25.0);
    let settings = fixture.ledger.settings().unwrap();
    assert_eq!(
        settings.as_of("reserve_soc_pct", at).unwrap().as_deref(),
        Some("25")
    );
    assert_eq!(
        settings
            .as_of("reserve_soc_pct", fixture.base)
            .unwrap()
            .as_deref(),
        Some("20")
    );
}
