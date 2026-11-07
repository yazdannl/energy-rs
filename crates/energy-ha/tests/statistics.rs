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

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use energy_ha::{
    Config, Connection, Granularity, StatRow, StatisticsResponse, parse_slots, slot_series,
    slot_series_15min,
};
use serde_json::json;
use support::{FakeServer, TOKEN};

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).expect("timestamp")
}

fn row(start: i64, change: Option<f64>) -> StatRow {
    StatRow {
        start,
        end: start + 300_000,
        change,
        ..StatRow::default()
    }
}

#[test]
fn first_row_is_discarded_as_warm_up() {
    let from = 1_800_000_000_000;
    let rows = vec![
        row(from - 300_000, Some(12_345.0)),
        row(from, Some(0.5)),
        row(from + 300_000, Some(0.25)),
    ];
    let slots = parse_slots(&rows, from, from + 600_000, Granularity::FiveMinute);
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].kwh, Some(0.5));
    assert_eq!(slots[1].kwh, Some(0.25));
}

#[test]
fn missing_and_negative_changes_are_gaps() {
    let from = 1_800_000_000_000;
    let rows = vec![
        row(from - 300_000, Some(100.0)),
        row(from, None),
        row(from + 300_000, Some(-1.0)),
        row(from + 600_000, Some(0.0)),
    ];
    let slots = parse_slots(&rows, from, from + 900_000, Granularity::FiveMinute);
    assert_eq!(slots[0].kwh, None);
    assert_eq!(slots[1].kwh, None);
    assert_eq!(slots[2].kwh, Some(0.0));
    assert!(slots[0].is_gap());
}

#[test]
fn quarter_hour_requires_all_three_buckets() {
    let from = 1_800_000_000_000;
    let rows = vec![
        row(from - 300_000, Some(9.0)),
        row(from, Some(0.1)),
        row(from + 300_000, Some(0.2)),
    ];
    let slots = parse_slots(&rows, from, from + 900_000, Granularity::QuarterHour);
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].kwh, None);

    let complete = vec![
        row(from - 300_000, Some(9.0)),
        row(from, Some(0.1)),
        row(from + 300_000, Some(0.2)),
        row(from + 600_000, Some(0.3)),
    ];
    let slots = parse_slots(&complete, from, from + 900_000, Granularity::QuarterHour);
    assert_eq!(slots[0].kwh, Some(0.600_000_000_000_000_1));
}

#[test]
fn recorded_five_minute_fixture_parses_without_zero_fill() {
    let response: StatisticsResponse =
        serde_json::from_value(support::result("statistics_5minute.json")).expect("stats");
    let (_, rows) = response
        .iter()
        .find(|(_, rows)| {
            rows.len() == 36
                && rows
                    .iter()
                    .skip(1)
                    .any(|row| row.change.is_some_and(|change| change > 0.0))
        })
        .expect("a 3-hour energy group");
    let from = rows[1].start;
    let to = rows.last().expect("rows").end;
    let slots = parse_slots(rows, from, to, Granularity::FiveMinute);
    assert_eq!(slots.len(), 35);
    for (slot, row) in slots.iter().zip(rows.iter().skip(1)) {
        assert_eq!(slot.start_ms, row.start);
        assert_eq!(slot.kwh, row.change);
    }
}

#[test]
fn recorded_hourly_fixture_parses() {
    let response: StatisticsResponse =
        serde_json::from_value(support::result("statistics_hourly.json")).expect("stats");
    let (_, rows) = response
        .iter()
        .find(|(_, rows)| rows.len() == 47)
        .expect("hourly group");
    let from = rows[1].start;
    let to = rows.last().expect("rows").end;
    let slots = parse_slots(rows, from, to, Granularity::Hour);
    assert_eq!(slots.len(), 46);
    assert_eq!(slots[0].start_ms, rows[1].start);
    assert_eq!(slots[0].kwh, rows[1].change);
}

#[tokio::test]
async fn slot_series_requests_a_warm_up_and_sums_fifteen_minutes() {
    let captured = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let capture = Arc::clone(&captured);
    let server = FakeServer::spawn(Arc::new(move |_, command, request| {
        if command != "recorder/statistics_during_period" {
            return support::Reply::fail("unknown_command", "unexpected command");
        }
        capture.lock().expect("capture").push(request.clone());
        let start = request["start_time"].as_str().expect("start");
        let parsed = DateTime::parse_from_rfc3339(start).expect("timestamp");
        let start_ms = parsed.timestamp_millis();
        support::Reply::ok(json!({
            "sensor.grid": [
                {"start": start_ms, "end": start_ms + 300_000, "change": 99_999.0},
                {"start": start_ms + 300_000, "end": start_ms + 600_000, "change": 0.5},
                {"start": start_ms + 600_000, "end": start_ms + 900_000, "change": 0.25},
                {"start": start_ms + 900_000, "end": start_ms + 1_200_000, "change": 0.25}
            ]
        }))
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Connection::connect(&config).await.expect("connect");
    let from = at(1_800_000_000);
    let to = from + Duration::minutes(15);

    let slots = slot_series_15min(&connection, "sensor.grid", from, to)
        .await
        .expect("slots");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].kwh, Some(1.0));

    let request = captured.lock().expect("capture")[0].clone();
    assert_eq!(request["period"], "5minute");
    assert_eq!(request["types"], json!(["change"]));
    assert_eq!(request["units"], json!({"energy": "kWh"}));
    let expected_start = (from - Duration::minutes(5)).to_rfc3339_opts(SecondsFormat::Secs, true);
    assert_eq!(request["start_time"], expected_start);
}

#[tokio::test]
async fn slot_series_is_empty_for_an_inverted_range() {
    let server = FakeServer::spawn(Arc::new(|_, _, _| support::Reply::ok(json!({})))).await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Connection::connect(&config).await.expect("connect");
    let from = at(1_800_000_000);
    let slots = slot_series(
        &connection,
        "sensor.grid",
        from,
        from,
        Granularity::QuarterHour,
    )
    .await
    .expect("slots");
    assert!(slots.is_empty());
    assert_eq!(server.requests(), 0);
}
