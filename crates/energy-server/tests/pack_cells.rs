//! Contract test for the cell-voltage object in `GET /api/v1/packs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use energy_core::stream::CellVoltages;
use support::{Fixture, get};

#[tokio::test]
async fn packs_carry_cells_for_a_report_and_nulls_for_an_empty_pack_data() {
    let fixture = Fixture::new();
    fixture.publish_cells(
        CellVoltages {
            min_cell_v: Some(3.32),
            max_cell_v: Some(3.35),
            spread_mv: Some(30.0),
            pack_voltage_v: Some(49.7),
            max_temp_c: Some(29.1),
        },
        CellVoltages::default(),
    );

    let (status, body) = get(&fixture.router(), "/api/v1/packs").await;
    assert_eq!(status, 200);
    let packs = body["packs"].as_array().unwrap();
    assert_eq!(packs.len(), 2);

    let reported = &packs[0]["state"]["cells"];
    assert_eq!(reported["minCellV"], 3.32);
    assert_eq!(reported["maxCellV"], 3.35);
    assert_eq!(reported["spreadMv"], 30.0);
    assert_eq!(reported["packVoltageV"], 49.7);
    assert_eq!(reported["maxTempC"], 29.1);
    // The cells share the state's existing freshness signal; no second clock is introduced.
    assert!(packs[0]["state"]["sampledAt"].is_string());
    assert_eq!(packs[0]["state"]["stale"], false);

    let empty = &packs[1]["state"]["cells"];
    assert!(empty["minCellV"].is_null());
    assert!(empty["maxCellV"].is_null());
    assert!(empty["spreadMv"].is_null());
    assert!(empty["packVoltageV"].is_null());
    assert!(empty["maxTempC"].is_null());
    assert_eq!(packs[1]["state"]["socPct"], 60.0);
}

#[tokio::test]
async fn cells_are_raw_readings_without_a_guard_verdict() {
    let fixture = Fixture::new();
    fixture.publish_cells(
        CellVoltages {
            min_cell_v: Some(2.99),
            max_cell_v: Some(3.32),
            spread_mv: Some(330.0),
            ..CellVoltages::default()
        },
        CellVoltages::default(),
    );

    let (_, body) = get(&fixture.router(), "/api/v1/packs").await;
    let packs = body["packs"].as_array().unwrap();
    let cells = packs[0]["state"]["cells"].as_object().unwrap();
    assert_eq!(cells["minCellV"], 2.99);
    assert_eq!(cells["spreadMv"], 330.0);
    assert!(
        !cells.contains_key("level"),
        "no voltage-only guard verdict"
    );
    // Blocking stays the latch's statement, including its hysteresis.
    assert_eq!(packs[0]["latch"]["blocked"], true);
}
