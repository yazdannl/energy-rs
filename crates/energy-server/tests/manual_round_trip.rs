//! Contract tests for the operator's per-pack round trip: PATCH write/clear/validation and the
//! effective value plus provenance reported by `GET /api/v1/packs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use axum::Router;
use serde_json::{Value, json};
use support::{Fixture, get, patch};

/// The second pack has no live snapshot, so ledger provenance is unambiguous.
const SN: &str = "AB2000-TWO";

fn uri() -> String {
    format!("/api/v1/packs/{SN}")
}

async fn pack_body(router: &Router, sn: &str) -> Value {
    let (status, body) = get(router, "/api/v1/packs").await;
    assert_eq!(status, 200);
    body["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .find(|pack| pack["sn"] == sn)
        .expect("the pack is listed")
        .clone()
}

#[tokio::test]
async fn patch_sets_clears_and_persists_manual_round_trip() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let before = pack_body(&router, SN).await;
    assert_eq!(before["roundTripSource"], "default");
    assert_eq!(before["manualRoundTrip"], Value::Null);

    let (status, body) = patch(&router, &uri(), json!({ "roundTripPct": 83.0 })).await;
    assert_eq!(status, 200);
    assert_eq!(body["sn"], SN);
    assert_eq!(body["manualRoundTrip"], 0.83);
    assert_eq!(body["manualRoundTripPct"], 83.0);

    let stored = pack_body(&router, SN).await;
    assert_eq!(stored["roundTripSource"], "manual");
    assert_eq!(stored["roundTrip"], 0.83);
    assert!((stored["etaCharge"].as_f64().unwrap() - 0.83_f64.sqrt()).abs() < 1e-12);
    assert_eq!(stored["manualRoundTrip"], 0.83);

    let (status, body) = patch(&router, &uri(), json!({ "roundTripPct": null })).await;
    assert_eq!(status, 200);
    assert_eq!(body["manualRoundTrip"], Value::Null);
    assert_eq!(body["manualRoundTripPct"], Value::Null);

    let cleared = pack_body(&router, SN).await;
    assert_eq!(cleared["roundTripSource"], "default");
    assert_eq!(cleared["roundTrip"], 0.81);
    assert_eq!(cleared["manualRoundTrip"], Value::Null);
}

#[tokio::test]
async fn patch_rejects_out_of_range_and_non_numeric_percentages() {
    let fixture = Fixture::new();
    let router = fixture.router();
    for bad in [json!(40.0), json!(120.0), json!("NaN"), json!("Infinity")] {
        let (status, body) = patch(&router, &uri(), json!({ "roundTripPct": bad })).await;
        assert_eq!(status, 422, "{bad}");
        assert_eq!(body["error"]["code"], "validation_failed");
        assert_eq!(body["error"]["details"]["key"], "roundTripPct");
        assert_eq!(body["error"]["details"]["min"], 50.0);
        assert_eq!(body["error"]["details"]["max"], 100.0);
    }
    let (status, body) = patch(&router, &uri(), json!({})).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");

    let (status, _) = patch(&router, &uri(), json!({ "roundTripPct": 83.0 })).await;
    assert_eq!(status, 200);
    let (status, _) = patch(&router, &uri(), json!({ "roundTripPct": 120.0 })).await;
    assert_eq!(status, 422);
    let after = pack_body(&router, SN).await;
    assert_eq!(after["roundTripSource"], "manual");
    assert_eq!(after["roundTrip"], 0.83);
}

#[tokio::test]
async fn packs_reports_learned_manual_and_default_sources() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let id = fixture
        .ledger
        .packs()
        .expect("packs")
        .by_sn(SN)
        .expect("read")
        .expect("a pack row")
        .id;

    // A live profile with a published measurement wins over a manual entry on the other pack.
    let live = pack_body(&router, &fixture.pack_sn).await;
    assert_eq!(live["learned"]["roundTripSource"], "measured");
    assert_eq!(live["roundTripSource"], "learned");
    assert_eq!(live["roundTrip"], 0.9);

    let (status, _) = patch(&router, &uri(), json!({ "roundTripPct": 83.0 })).await;
    assert_eq!(status, 200);
    let manual = pack_body(&router, SN).await;
    assert_eq!(manual["roundTripSource"], "manual");
    assert_eq!(manual["roundTrip"], 0.83);

    // Publishing a measurement keeps the operator's entry visible but stops using it.
    fixture
        .ledger
        .packs()
        .expect("packs")
        .set_learning(id, None, 0.92, 0.8, 1)
        .expect("learning is published");
    let learned = pack_body(&router, SN).await;
    assert_eq!(learned["roundTripSource"], "learned");
    assert_eq!(learned["roundTrip"], 0.92);
    assert_eq!(learned["manualRoundTrip"], 0.83);
    assert_eq!(learned["manualRoundTripPct"], 83.0);

    // An unmeasured pass writes the typed seed with a never-measured stamp: it is not a measurement.
    fixture
        .ledger
        .packs()
        .expect("packs")
        .set_learning(id, None, 0.81, 0.0, 0)
        .expect("an unmeasured pass");
    let unmeasured = pack_body(&router, SN).await;
    assert_eq!(unmeasured["roundTripSource"], "manual");
    assert_eq!(unmeasured["roundTrip"], 0.83);
}

#[tokio::test]
async fn manual_round_trip_percent_round_trips_exactly() {
    let fixture = Fixture::new();
    let router = fixture.router();
    for (percent, fraction) in [(81.0, 0.81), (50.0, 0.5), (100.0, 1.0), (77.5, 0.775)] {
        let (status, _) = patch(&router, &uri(), json!({ "roundTripPct": percent })).await;
        assert_eq!(status, 200);
        let pack = pack_body(&router, SN).await;
        assert_eq!(pack["manualRoundTripPct"], percent);
        assert_eq!(pack["roundTrip"], fraction);
        assert_eq!(pack["roundTripSource"], "manual");
    }
}
