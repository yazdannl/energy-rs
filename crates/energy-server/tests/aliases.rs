//! Contract tests for `PATCH /api/v1/packs/{sn}`: the operator's alias, its validation and the
//! way it shows up in `/site`, `/packs` and pack lookup.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use serde_json::json;
use support::{Fixture, get, patch, post};

#[tokio::test]
async fn an_alias_is_written_and_shown_by_site_and_packs() {
    let fixture = Fixture::new();
    let uri = format!("/api/v1/packs/{}", fixture.pack_sn);
    let (status, body) = patch(&fixture.router(), &uri, json!({ "alias": " Attic A " })).await;
    assert_eq!(status, 200);
    assert_eq!(body["sn"], fixture.pack_sn);
    // The label is stored trimmed.
    assert_eq!(body["alias"], "Attic A");

    let (status, site) = get(&fixture.router(), "/api/v1/site").await;
    assert_eq!(status, 200);
    assert_eq!(site["packs"][0]["alias"], "Attic A");
    // The discovered name is the other half of the identity and is not touched.
    assert_eq!(site["packs"][0]["name"], serde_json::Value::Null);

    let (status, packs) = get(&fixture.router(), "/api/v1/packs").await;
    assert_eq!(status, 200);
    assert_eq!(packs["packs"][0]["alias"], "Attic A");
}

#[tokio::test]
async fn a_null_or_blank_alias_clears_the_label() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let uri = format!("/api/v1/packs/{}", fixture.pack_sn);

    let (status, _) = patch(&router, &uri, json!({ "alias": "Attic A" })).await;
    assert_eq!(status, 200);

    let (status, body) = patch(&router, &uri, json!({ "alias": null })).await;
    assert_eq!(status, 200);
    assert_eq!(body["alias"], serde_json::Value::Null);

    let (status, _) = patch(&router, &uri, json!({ "alias": "Attic A" })).await;
    assert_eq!(status, 200);
    let (status, body) = patch(&router, &uri, json!({ "alias": "   " })).await;
    assert_eq!(status, 200);
    assert_eq!(body["alias"], serde_json::Value::Null);
}

#[tokio::test]
async fn an_alias_is_unique_and_a_missing_key_is_refused() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let first = format!("/api/v1/packs/{}", fixture.pack_sn);
    let second = "/api/v1/packs/AB2000-TWO";

    let (status, _) = patch(&router, &first, json!({ "alias": "Attic A" })).await;
    assert_eq!(status, 200);
    let (status, body) = patch(&router, second, json!({ "alias": "Attic A" })).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["details"]["sn"], fixture.pack_sn);

    // Repeating the same alias on the pack that holds it is fine.
    let (status, _) = patch(&router, &first, json!({ "alias": "Attic A" })).await;
    assert_eq!(status, 200);

    let (status, body) = patch(&router, &first, json!({})).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");

    let long = "x".repeat(41);
    let (status, body) = patch(&router, &first, json!({ "alias": long })).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["details"]["max"], 40);

    let (status, body) = patch(&router, &first, json!({ "alias": "a\nb" })).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn an_unknown_pack_is_404_and_a_command_accepts_the_alias() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let uri = format!("/api/v1/packs/{}", fixture.pack_sn);
    let (status, _) = patch(&router, &uri, json!({ "alias": "Attic A" })).await;
    assert_eq!(status, 200);

    let (status, body) = patch(&router, "/api/v1/packs/NOPE", json!({ "alias": "x" })).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "unknown_pack");

    // The alias is another way to name a pack in a command; the answer echoes the serial.
    let (status, body) = post(
        &router,
        "/api/v1/override",
        json!({ "pack": "Attic A", "action": "hold" }),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(body["pack"], fixture.pack_sn);
}
