//! Error-envelope mapping and redaction: every failure is `{error:{code,message,details?}}` and no
//! response ever contains a token, a secret or a file path.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::http::Method;
use serde_json::json;
use support::{Fixture, empty_router, get, send_text};

use energy_ledger::Plan;

#[tokio::test]
async fn an_unknown_api_path_is_a_json_404() {
    let (status, body) = get(&empty_router(), "/api/v1/nope").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "not_found");
    assert!(body["error"]["message"].is_string());
}

#[tokio::test]
async fn a_malformed_json_body_is_400_invalid_body() {
    let fixture = Fixture::new();
    let (status, text) = send_text(
        &fixture.router(),
        Method::POST,
        "/api/v1/override",
        Some(json!({ "pack": 12 })),
    )
    .await;
    assert_eq!(status, 400);
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "invalid_body");
}

#[tokio::test]
async fn a_missing_plan_is_404_and_a_corrupt_plan_is_500_without_leaking_the_path() {
    let (status, body) = get(&empty_router(), "/api/v1/plan/current").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "not_found");

    let fixture = Fixture::new();
    fixture
        .ledger
        .plans()
        .unwrap()
        .insert(
            &Plan::new(
                fixture.base + 1,
                "dry_run",
                fixture.base,
                fixture.base,
                "{not json",
            ),
            &[],
        )
        .unwrap();
    let (status, text) =
        send_text(&fixture.router(), Method::GET, "/api/v1/plan/current", None).await;
    assert_eq!(status, 500);
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "corrupt_plan");
    assert!(!text.contains(&fixture.dir.path().display().to_string()));
}

#[tokio::test]
async fn no_response_contains_a_secret_or_a_file_path() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let uris = [
        "/api/v1/health",
        "/api/v1/site",
        "/api/v1/packs",
        "/api/v1/plans?limit=5",
        "/api/v1/dispatch?limit=5",
        "/api/v1/savings?period=lifetime",
        "/api/v1/settings",
        "/api/v1/plan/current",
        "/api/v1/nope",
    ];
    let path = fixture.dir.path().display().to_string();
    for uri in uris {
        let (_, text) = send_text(&router, Method::GET, uri, None).await;
        let lower = text.to_lowercase();
        assert!(
            !text.contains("SUPERSECRET"),
            "{uri} leaked the private value"
        );
        assert!(
            !lower.contains("refresh_token"),
            "{uri} leaked a private key"
        );
        assert!(
            !lower.contains("password"),
            "{uri} leaked a credential name"
        );
        assert!(
            !lower.contains("supervisor_token"),
            "{uri} leaked a token name"
        );
        assert!(!text.contains(&path), "{uri} leaked the database path");
    }
    // A rejected write must be just as silent.
    let (_, text) = send_text(
        &router,
        Method::POST,
        "/api/v1/override",
        Some(json!({ "pack": "nope", "action": "charge" })),
    )
    .await;
    assert!(!text.contains("SUPERSECRET"));
    assert!(!text.contains(&path));
}
