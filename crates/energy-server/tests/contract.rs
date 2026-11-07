//! Contract tests for the read routes: status, keys, units and values from a seeded ledger.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use chrono::{DateTime, SecondsFormat};
use support::{Fixture, get};

fn stamp(epoch: i64) -> String {
    DateTime::from_timestamp(epoch, 0)
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[tokio::test]
async fn health_reports_liveness_mode_and_last_tick() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/health").await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["mode"], "dry_run");
    assert!(body["lastTickAt"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn site_lists_sources_capabilities_and_packs() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/site").await;
    assert_eq!(status, 200);
    assert_eq!(body["configured"], true);
    assert_eq!(body["capabilities"][0], "battery_control");
    assert_eq!(body["sources"][0]["kind"], "grid");
    assert_eq!(body["sources"][1]["entityId"], "sensor.pv_energy");
    assert_eq!(body["packs"].as_array().unwrap().len(), 2);
    assert_eq!(body["packs"][0]["sn"], "AB2000-ONE");
    assert_eq!(body["packs"][0]["labelCapacityKwh"], 1.92);
    assert_eq!(body["packs"][0]["maxChargeW"], 800.0);
    assert_eq!(body["mode"], "dry_run");
}

#[tokio::test]
async fn packs_join_state_learning_latch_and_limits() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/packs").await;
    assert_eq!(status, 200);
    let packs = body["packs"].as_array().unwrap();
    assert_eq!(packs.len(), 2);

    let first = &packs[0];
    assert_eq!(first["sn"], "AB2000-ONE");
    assert_eq!(first["state"]["socPct"], 60.0);
    assert_eq!(first["state"]["stale"], false);
    assert_eq!(first["learned"]["capacityKwh"], 1.85);
    assert_eq!(first["learned"]["capacityDecision"], "capacity_learned");
    assert_eq!(first["learned"]["sessionsUsed"], 5);
    assert_eq!(first["latch"]["blocked"], true);
    assert_eq!(first["latch"]["reason"], "cell_below_resume");
    assert_eq!(first["limits"]["maxDischargeW"], 800.0);
    assert!(first["override"].is_null());

    let second = &packs[1];
    assert_eq!(second["sn"], "AB2000-TWO");
    assert!(second["learned"].is_null());
    assert!(second["latch"].is_null());
    assert_eq!(second["state"]["socPct"], 45.0);
}

#[tokio::test]
async fn prices_are_eur_per_kwh_on_the_published_grid() {
    let fixture = Fixture::new();
    let from = stamp(fixture.base);
    let to = stamp(fixture.base + 48 * 3_600);
    let uri = format!("/api/v1/prices?from={from}&to={to}");
    let (status, body) = get(&fixture.router(), &uri).await;
    assert_eq!(status, 200);
    assert_eq!(body["currency"], "EUR");
    assert_eq!(body["count"], 192);
    let slots = body["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 192);
    assert_eq!(slots[0]["buyEurPerKwh"], 0.2);
    assert_eq!(slots[0]["source"], "zonneplan_quarter_hourly");
    assert!(slots[0]["feedInEurPerKwh"].as_f64().unwrap() >= 0.0);
    assert!(slots[0]["startAt"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn prices_reject_an_inverted_or_too_wide_window() {
    let fixture = Fixture::new();
    let at = stamp(fixture.base);
    let uri = format!("/api/v1/prices?from={at}&to={at}");
    let (status, body) = get(&fixture.router(), &uri).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_query");

    let (status, body) = get(&fixture.router(), "/api/v1/prices?from=not-a-time").await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_query");

    let far = stamp(fixture.base + 30 * 24 * 3_600);
    let uri = format!("/api/v1/prices?from={at}&to={far}");
    let (status, body) = get(&fixture.router(), &uri).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_query");
}

#[tokio::test]
async fn current_plan_renders_slots_in_w_and_wh() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/plan/current").await;
    assert_eq!(status, 200);
    assert_eq!(body["planId"], fixture.plan_id);
    assert_eq!(body["mode"], "dry_run");
    assert_eq!(body["feasible"], true);
    assert_eq!(body["summary"]["costEur"], 1.234_568);
    assert_eq!(body["summary"]["chargedWh"], 2_500.0);
    assert_eq!(body["bindingReasons"][0], "price_low");

    let slots = body["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 4);
    assert_eq!(slots[0]["idx"], 0);
    assert_eq!(slots[0]["pvWh"], 250.0);
    assert_eq!(slots[0]["loadWh"], 125.0);
    assert_eq!(slots[0]["batteryPowerW"], 1_500.0);
    assert_eq!(slots[0]["socWh"], 1_000.0);
    assert_eq!(slots[0]["targetSocWh"], 1_250.0);
    assert_eq!(slots[0]["reason"], "price_low");
    assert!(slots[0]["endAt"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn plans_lists_newest_first_and_honours_limit() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/plans?limit=1").await;
    assert_eq!(status, 200);
    assert_eq!(body["count"], 1);
    assert_eq!(body["plans"][0]["planId"], fixture.plan_id);
    assert_eq!(body["plans"][0]["feasible"], true);
    assert_eq!(body["plans"][0]["savedEur"], 0.765_432);

    let (status, body) = get(&fixture.router(), "/api/v1/plans?limit=0").await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_query");
}

#[tokio::test]
async fn dispatch_lists_newest_first_with_pack_serial() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/dispatch?limit=10").await;
    assert_eq!(status, 200);
    assert_eq!(body["count"], 2);
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries[0]["packSn"], "AB2000-ONE");
    assert_eq!(entries[0]["requestedW"], -400.0);
    assert_eq!(entries[0]["appliedW"], -395.0);
    assert_eq!(entries[0]["verified"], true);
    assert_eq!(entries[1]["verified"], false);
    assert_eq!(entries[1]["mode"], "dry_run");
}

#[tokio::test]
async fn savings_roll_up_days_and_totals() {
    let fixture = Fixture::new();
    let (status, body) = get(&fixture.router(), "/api/v1/savings?period=day").await;
    assert_eq!(status, 200);
    assert_eq!(body["period"], "day");
    assert_eq!(body["days"].as_array().unwrap().len(), 1);
    assert_eq!(body["days"][0]["actualEur"], 0.5);
    assert_eq!(body["days"][0]["complete"], false);
    assert_eq!(body["days"][0]["coverageHours"], 6.0);
    assert_eq!(body["totals"]["savingEur"], 0.5);
    assert_eq!(body["totals"]["completeDays"], 0);

    let (status, body) = get(&fixture.router(), "/api/v1/savings?period=lifetime").await;
    assert_eq!(status, 200);
    assert_eq!(body["days"].as_array().unwrap().len(), 3);
    let totals = &body["totals"];
    // Lifetime counts complete days only: the provisional today and the measured gap stay out.
    assert_eq!(totals["actualEur"], 1.5);
    assert_eq!(totals["baselineEur"], 2.5);
    assert_eq!(totals["savingEur"], 1.0);
    assert_eq!(totals["arbitrageEur"], 0.4);
    assert_eq!(totals["selfConsumptionEur"], 0.5);
    assert_eq!(totals["feedInEur"], 0.2);
    assert_eq!(totals["batteryBalanceEur"], -0.1);
    assert_eq!(totals["importWh"], 4_000.0);
    assert_eq!(totals["completeDays"], 1);
    let gap = body["days"]
        .as_array()
        .unwrap()
        .iter()
        .find(|day| day["complete"] == false && day["coverageHours"] == 3.0)
        .expect("the incomplete day is still listed");
    assert!(gap["savingEur"].is_null());
    assert!(gap["actualEur"].is_null());

    let (status, body) = get(&fixture.router(), "/api/v1/savings?period=week").await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "invalid_query");
}
