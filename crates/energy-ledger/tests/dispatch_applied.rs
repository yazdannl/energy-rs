//! Applied watts are a measurement, never a placeholder zero.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

mod support;

use energy_ledger::{DispatchEntry, MeasuredWatts};
use support::{TempDatabase, register_pack};

#[test]
fn unmeasured_rows_round_trip_as_null_and_a_measured_row_as_a_number() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let log = ledger.dispatch().unwrap();
    for (ts, reason, entry) in [
        (
            100,
            "sample_stale",
            DispatchEntry::new(100, Some(pack), 900.0, "watchdog_hold", "auto")
                .echo_verified()
                .written(true)
                .with_applied_w_reason("sample_stale"),
        ),
        (
            200,
            "confirmation_pending",
            DispatchEntry::new(200, Some(pack), 900.0, "dispatch", "auto")
                .written(true)
                .with_applied_w_reason("confirmation_pending"),
        ),
        (
            300,
            "read_failed",
            DispatchEntry::new(300, Some(pack), 900.0, "dispatch", "auto")
                .with_applied_w_reason("read_failed"),
        ),
    ] {
        let id = log.insert(&entry).unwrap();
        let rows = log.range(ts, ts + 1).unwrap();
        assert_eq!(rows.len(), 1, "row {id}");
        assert_eq!(rows[0].applied_w(), None, "{reason}");
        assert_eq!(rows[0].applied_w_reason(), Some(reason));
        assert!(!rows[0].flow_measured(), "{reason}");
    }
    log.insert(
        &DispatchEntry::new(400, Some(pack), 900.0, "dispatch", "auto")
            .measured(MeasuredWatts::new(795.0, 1))
            .verified(),
    )
    .unwrap();
    let measured = log.range(400, 401).unwrap();
    assert_eq!(measured[0].applied_w(), Some(795.0));
    assert!(measured[0].flow_measured());
}

#[test]
fn contradictory_builder_orders_are_normalised() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let sample = MeasuredWatts::new(795.0, 100);

    let stale_then_measured = DispatchEntry::new(100, Some(pack), 800.0, "dispatch", "auto")
        .with_applied_w_reason("sample_stale")
        .measured(sample);
    assert_eq!(stale_then_measured.applied_w(), Some(795.0));
    assert!(stale_then_measured.flow_measured());
    assert_eq!(stale_then_measured.applied_w_reason(), None);

    let measured_then_stale = DispatchEntry::new(110, Some(pack), 800.0, "dispatch", "auto")
        .measured(sample)
        .with_applied_w_reason("sample_stale");
    assert_eq!(measured_then_stale.applied_w(), None);
    assert!(!measured_then_stale.flow_measured());

    let claimed = DispatchEntry::new(120, Some(pack), 800.0, "dispatch", "auto");
    assert_eq!(claimed.applied_w(), None, "no sample, no number");
    assert!(!claimed.flow_measured(), "no sample, no measurement status");

    let log = ledger.dispatch().unwrap();
    log.insert(&stale_then_measured).unwrap();
    let rows = log.range(100, 101).unwrap();
    assert_eq!(rows[0].applied_w(), Some(795.0));
    assert!(rows[0].flow_measured());
    assert_eq!(rows[0].applied_w_reason(), None);
}

#[test]
fn flow_verdict_clocks_round_trip() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let log = ledger.dispatch().unwrap();
    let entry = DispatchEntry::new(1_700_000_000, None, 1000.0, "watchdog_hold", "auto")
        .with_flow_verdict(1_700_000_046, 1_700_000_000);
    log.insert(&entry).unwrap();
    let rows = log.range(1_700_000_000, 1_700_000_001).unwrap();
    assert_eq!(
        rows[0].flow_verdict(),
        (Some(1_700_000_046), Some(1_700_000_000))
    );
}
