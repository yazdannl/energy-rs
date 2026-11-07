//! Recorded-payload tests: the two provider shapes, the gas decoy, the 1e7 fixed point, past-slot
//! filtering, gap refusal and coverage-based staleness.  No network: every payload here is a
//! fixture ingested exactly like a live one.

// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here, and fixture timestamps/money read better verbatim than with digit separators.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::unreadable_literal
)]

mod support;

use energy_price::book::query::PriceQuery;
use energy_price::provider::{ProviderKind, ingest, parse_chart, parse_state};
use serde_json::{Value, json};
use support::{contract, instant};

fn query(from: &str, to: &str, now: i64, horizon_secs: i64) -> PriceQuery {
    PriceQuery::new(support::date(from), support::date(to), now, horizon_secs)
        .expect("the fixture query is valid")
}

fn before(rfc3339: &str) -> i64 {
    instant(rfc3339) - 1
}

/// Quarter-hourly chart entries for four consecutive slots from local midnight.
fn quarter_hour_entries() -> Value {
    let mut prices = Vec::new();
    for (start, end) in [
        ("00:00", "00:15"),
        ("00:15", "00:30"),
        ("00:30", "00:45"),
        ("00:45", "01:00"),
    ] {
        prices.push(json!({
            "start_date": format!("2026-11-20T{start}:00+01:00"),
            "end_date": format!("2026-11-20T{end}:00+01:00"),
            "price_tax_included": {"amount": 2_000_000},
            "sustainability_score": {"permille": 812},
        }));
    }
    json!({"chart": {"series": {"prices": prices}}})
}

#[test]
fn a_quarter_hourly_chart_becomes_a_netted_price_book() {
    let parsed = parse_chart(&quarter_hour_entries()).expect("the chart is electricity");
    assert_eq!(parsed.step, 900);
    assert_eq!(parsed.raw.len(), 4);

    let book = ingest(
        parsed,
        ProviderKind::Zonneplan,
        &contract(),
        &query(
            "2026-11-20",
            "2026-11-20",
            before("2026-11-20T00:00:00+01:00"),
            3600,
        ),
    )
    .expect("the payload is a valid book");
    assert_eq!(book.len(), 4);
    let first = book.first().expect("a non-empty book");
    assert_eq!(first.buy_all_in, 0.20);
    // The fixed point is divided by 1e7 and the market price is derived at ingest.
    assert_eq!(first.market, 0.057150);
    assert_eq!(
        first.source,
        energy_price::SlotSource::ZonneplanQuarterHourly
    );
    assert_eq!(first.t_start, instant("2026-11-20T00:00:00+01:00"));
    assert_eq!(
        book.slot_at(first.t_start + 900)
            .map(|slot| slot.buy_all_in),
        Some(0.20)
    );
}

#[test]
fn the_legacy_hourly_chart_is_accepted_as_its_own_resolution() {
    let payload = json!({"chart": {"series": {"prices": [
        {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-20T01:00:00+01:00",
         "price_tax_included": {"amount": 1_500_000}},
        {"start_date": "2026-11-20T01:00:00+01:00", "end_date": "2026-11-20T02:00:00+01:00",
         "price_tax_included": {"amount": 3_000_000}},
    ]}}});
    let parsed = parse_chart(&payload).expect("hourly is still a price grid");
    assert_eq!(parsed.step, 3600);
    let book = ingest(
        parsed,
        ProviderKind::Zonneplan,
        &contract(),
        &query("2026-11-20", "2026-11-20", 0, 0),
    )
    .expect("the hourly payload is a valid book");
    assert_eq!(book.len(), 2);
    assert_eq!(
        book.first().map(|slot| slot.source),
        Some(energy_price::SlotSource::ZonneplanHourly)
    );
    assert_eq!(book.first().map(|slot| slot.buy_all_in), Some(0.15));
}

#[test]
fn the_gas_tariff_is_rejected_in_both_chart_shapes() {
    // One entry spanning a whole day: the gas sensor's shape, never an electricity grid.
    let single = json!({"chart": {"series": {"prices": [
        {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-21T00:00:00+01:00",
         "price_tax_included": {"amount": 1_234_560}},
    ]}}});
    assert!(parse_chart(&single).is_err());

    // Even a 24-hour grid of two entries is not a settlement grid.
    let daily = json!({"chart": {"series": {"prices": [
        {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-21T00:00:00+01:00",
         "price_tax_included": {"amount": 1_234_560}},
        {"start_date": "2026-11-21T00:00:00+01:00", "end_date": "2026-11-22T00:00:00+01:00",
         "price_tax_included": {"amount": 1_234_560}},
    ]}}});
    assert!(parse_chart(&daily).is_err());

    // The HA gas sensor is caught by its unit before the grid test runs.
    let gas_sensor = json!({
        "state": "1.42",
        "attributes": {"unit_of_measurement": "€/m³", "forecast": [
            {"start_date": "2026-11-20T00:00:00+01:00",
             "price_tax_included": {"amount": 1_234_560}},
        ]},
    });
    assert!(parse_state(&gas_sensor).is_err());
}

#[test]
fn a_mixed_grid_and_a_non_electricity_unit_are_refused() {
    let mixed = json!({"chart": {"series": {"prices": [
        {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-20T00:15:00+01:00",
         "price_tax_included": {"amount": 2_000_000}},
        {"start_date": "2026-11-20T00:15:00+01:00", "end_date": "2026-11-20T01:15:00+01:00",
         "price_tax_included": {"amount": 2_000_000}},
    ]}}});
    assert!(parse_chart(&mixed).is_err());

    let wrong_unit = json!({
        "state": "20.0",
        "attributes": {"unit_of_measurement": "EUR/MWh", "forecast": []},
    });
    assert!(parse_state(&wrong_unit).is_err());
}

#[test]
fn the_ha_quarter_hourly_sensor_forecast_parses() {
    let payload = json!({
        "state": "0.20",
        "attributes": {
            "unit_of_measurement": "€/kWh",
            "forecast": [
                {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-20T00:15:00+01:00",
                 "price_tax_included": {"amount": 2_000_000},
                 "price_tax_excluded": {"amount": 1_123_450}},
                {"start_date": "2026-11-20T00:15:00+01:00", "end_date": "2026-11-20T00:30:00+01:00",
                 "price_tax_included": {"amount": 2_500_000}},
            ],
        },
    });
    let book = ingest(
        parse_state(&payload).expect("the sensor is electricity"),
        ProviderKind::Ha,
        &contract(),
        &query("2026-11-20", "2026-11-20", 0, 0),
    )
    .expect("the forecast is a valid book");
    assert_eq!(book.len(), 2);
    assert_eq!(
        book.first().map(|slot| slot.source),
        Some(energy_price::SlotSource::HaQuarterHourly)
    );
    assert_eq!(book.last().map(|slot| slot.buy_all_in), Some(0.25));
}

#[test]
fn the_legacy_hourly_sensor_shape_parses_from_datetime_and_electricity_price() {
    let payload = json!({
        "state": "0.20",
        "attributes": {
            "unit_of_measurement": "€/kWh",
            "forecast": [
                {"datetime": "2026-11-20T00:00:00+01:00", "electricity_price": 1_800_000},
                {"datetime": "2026-11-20T01:00:00+01:00", "electricity_price": 2_400_000},
            ],
        },
    });
    let parsed = parse_state(&payload).expect("the sensor is electricity");
    assert_eq!(parsed.step, 3600);
    let book = ingest(
        parsed,
        ProviderKind::Ha,
        &contract(),
        &query("2026-11-20", "2026-11-20", 0, 0),
    )
    .expect("the forecast is a valid book");
    assert_eq!(
        book.first().map(|slot| slot.source),
        Some(energy_price::SlotSource::HaHourly)
    );
    assert_eq!(book.first().map(|slot| slot.buy_all_in), Some(0.18));
}

#[test]
fn an_already_scaled_amount_is_not_divided_a_second_time() {
    let scaled = json!({"chart": {"series": {"prices": [
        {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-20T00:15:00+01:00",
         "price_tax_included": {"amount": 0.204321}},
        {"start_date": "2026-11-20T00:15:00+01:00", "end_date": "2026-11-20T00:30:00+01:00",
         "price_tax_included": {"amount": -2_043_210}},
    ]}}});
    let book = ingest(
        parse_chart(&scaled).expect("the chart is electricity"),
        ProviderKind::Zonneplan,
        &contract(),
        &query("2026-11-20", "2026-11-20", 0, 0),
    )
    .expect("the chart is a valid book");
    assert_eq!(book.first().map(|slot| slot.buy_all_in), Some(0.204321));
    // A raw negative amount is still fixed point, and negative prices are allowed.
    assert_eq!(book.last().map(|slot| slot.buy_all_in), Some(-0.204321));
}

#[test]
fn past_slots_are_dropped_and_the_remaining_grid_stays_contiguous() {
    let now = instant("2026-11-20T00:30:00+01:00");
    let book = ingest(
        parse_chart(&quarter_hour_entries()).expect("electricity"),
        ProviderKind::Zonneplan,
        &contract(),
        &query("2026-11-20", "2026-11-20", now, 0),
    )
    .expect("the remaining slots are a valid book");
    // The 00:15-00:30 slot is over at `now`; 00:30 and 00:45 remain.
    assert_eq!(book.len(), 2);
    assert_eq!(book.first().map(|slot| slot.t_start), Some(now));
}

#[test]
fn a_hole_in_the_payload_is_refused() {
    let payload = json!({"chart": {"series": {"prices": [
        {"start_date": "2026-11-20T00:00:00+01:00", "end_date": "2026-11-20T00:15:00+01:00",
         "price_tax_included": {"amount": 2_000_000}},
        {"start_date": "2026-11-20T00:15:00+01:00", "end_date": "2026-11-20T00:30:00+01:00",
         "price_tax_included": {"amount": 2_000_000}},
        {"start_date": "2026-11-20T00:45:00+01:00", "end_date": "2026-11-20T01:00:00+01:00",
         "price_tax_included": {"amount": 2_000_000}},
    ]}}});
    let error = ingest(
        parse_chart(&payload).expect("the entries parse"),
        ProviderKind::Zonneplan,
        &contract(),
        &query("2026-11-20", "2026-11-20", 0, 0),
    )
    .expect_err("a hole is exact-or-refuse");
    assert!(matches!(
        error,
        energy_price::PriceError::Gap { t_start } if t_start == instant("2026-11-20T00:30:00+01:00")
    ));
}

#[test]
fn a_payload_without_the_requested_date_is_not_published() {
    let error = ingest(
        parse_chart(&quarter_hour_entries()).expect("electricity"),
        ProviderKind::Zonneplan,
        &contract(),
        &query("2026-11-21", "2026-11-21", 0, 0),
    )
    .expect_err("tomorrow is not in this payload");
    assert!(matches!(
        error,
        energy_price::PriceError::NotPublished { .. }
    ));
}

#[test]
fn coverage_staleness_is_decided_by_the_book_not_by_the_sensor_state() {
    let book = ingest(
        parse_chart(&quarter_hour_entries()).expect("electricity"),
        ProviderKind::Zonneplan,
        &contract(),
        &query("2026-11-20", "2026-11-20", 0, 0),
    )
    .expect("a valid book");
    let now = instant("2026-11-20T00:00:00+01:00");
    // A slot covers `now`, so the book is usable with no horizon requirement.
    assert!(book.check_coverage(now, 0).is_ok());
    // 30 hours is beyond the four slots this publication covers: stale, not zero-filled.
    let error = book
        .check_coverage(now, 30 * 3600)
        .expect_err("coverage is too short");
    assert!(matches!(error, energy_price::PriceError::Stale { .. }));
    // An instant the book does not cover is equally refused.
    assert!(
        book.check_coverage(instant("2026-11-20T05:00:00+01:00"), 0)
            .is_err()
    );
}

#[test]
fn a_query_refuses_an_inverted_range_or_a_negative_horizon() {
    let from = support::date("2026-11-20");
    let to = support::date("2026-11-19");
    assert!(PriceQuery::new(from, to, 0, 0).is_err());
    assert!(PriceQuery::new(from, from, 0, -1).is_err());
    assert_eq!(PriceQuery::new(from, from, 0, 0).expect("valid").days(), 1);
}
