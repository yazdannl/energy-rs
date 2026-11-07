//! Ingestion is an upsert; the 5-minute tier is a mean of the 1-minute rows with explicit empty
//! buckets, and a recomputation after retention must not erase populated buckets.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use energy_ledger::{RetentionPolicy, SampleKind, Tier};
use support::{TempDatabase, register_pack, sample, write};

#[test]
fn re_ingesting_a_timestamp_replaces_the_row() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let measurements = ledger.measurements().unwrap();
    measurements
        .write(&sample(pack, 60, 100.0, Some(40.0)))
        .unwrap();
    measurements
        .write(&sample(pack, 60, 250.0, Some(41.5)).with_kind(SampleKind::Derived))
        .unwrap();
    let rows = measurements.range(Tier::OneMinute, pack, 0, 120).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].power_w, Some(250.0));
    assert_eq!(rows[0].soc_pct, Some(41.5));
    assert_eq!(rows[0].samples, 1);
}

#[test]
fn a_batch_keeps_the_last_value_per_timestamp() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let batch = [
        sample(pack, 60, 100.0, Some(40.0)),
        sample(pack, 120, 200.0, Some(41.0)),
        sample(pack, 60, 300.0, Some(42.0)),
    ];
    assert_eq!(
        ledger.measurements().unwrap().write_many(&batch).unwrap(),
        3
    );
    let rows = ledger
        .measurements()
        .unwrap()
        .range(Tier::OneMinute, pack, 0, 180)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].power_w, Some(300.0));
    assert_eq!(rows[1].power_w, Some(200.0));
}

#[test]
fn an_empty_batch_writes_nothing() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    assert_eq!(ledger.measurements().unwrap().write_many(&[]).unwrap(), 0);
}

#[test]
fn rollup_means_the_bucket_and_marks_empty_buckets_null() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 0, 100.0, Some(40.0)),
            sample(pack, 60, 300.0, Some(60.0)),
            sample(pack, 600, 500.0, Some(80.0)),
            sample(pack, 660, 700.0, Some(90.0)),
        ],
    );
    let report = ledger.measurements().unwrap().rollup(0, 900).unwrap();
    assert_eq!(report.buckets, 3);
    assert_eq!(report.rows, 3);
    let rows = ledger
        .measurements()
        .unwrap()
        .range(Tier::FiveMinute, pack, 0, 900)
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].ts, 0);
    assert_eq!(rows[0].power_w, Some(200.0));
    assert_eq!(rows[0].soc_pct, Some(50.0));
    assert_eq!(rows[0].samples, 2);
    assert_eq!(rows[1].ts, 300);
    assert_eq!(rows[1].power_w, None);
    assert_eq!(rows[1].soc_pct, None);
    assert_eq!(rows[1].samples, 0);
    assert_eq!(rows[2].power_w, Some(600.0));
    assert_eq!(rows[2].soc_pct, Some(85.0));
}

#[test]
fn rollup_is_idempotent_and_writes_every_pack() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let first = register_pack(&ledger, "ABC123");
    let second = register_pack(&ledger, "ABC456");
    write(&ledger, &[sample(first, 0, 100.0, Some(40.0))]);
    ledger.measurements().unwrap().rollup(0, 300).unwrap();
    ledger.measurements().unwrap().rollup(0, 300).unwrap();
    let rows = ledger
        .measurements()
        .unwrap()
        .range(Tier::FiveMinute, first, 0, 300)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].samples, 1);
    let idle = ledger
        .measurements()
        .unwrap()
        .range(Tier::FiveMinute, second, 0, 300)
        .unwrap();
    assert_eq!(idle.len(), 1);
    assert_eq!(idle[0].samples, 0);
    assert_eq!(idle[0].power_w, None);
}

#[test]
fn rollup_after_retention_leaves_populated_buckets_alone() {
    let db = TempDatabase::new();
    let retention = RetentionPolicy::new(0, 365, 200, 20_000);
    let ledger = db.ledger_with(retention);
    let pack = register_pack(&ledger, "ABC123");
    // The samples sit in yesterday's local day (today starts at 1_781_474_400): the 1-minute
    // policy of 0 days deletes them, while the 5-minute row stays inside the one-year window.
    let start = 1_781_474_400 - 3_600;
    write(
        &ledger,
        &[
            sample(pack, start, 100.0, Some(40.0)),
            sample(pack, start + 60, 300.0, Some(60.0)),
        ],
    );
    ledger
        .measurements()
        .unwrap()
        .rollup(start, start + 300)
        .unwrap();
    ledger.run_retention(1_781_517_600).unwrap();
    ledger
        .measurements()
        .unwrap()
        .rollup(start, start + 300)
        .unwrap();
    let rows = ledger
        .measurements()
        .unwrap()
        .range(Tier::FiveMinute, pack, start, start + 300)
        .unwrap();
    assert_eq!(rows[0].power_w, Some(200.0));
    assert_eq!(rows[0].samples, 2);
}

#[test]
fn rollup_ignores_an_empty_range() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    register_pack(&ledger, "ABC123");
    let report = ledger.measurements().unwrap().rollup(300, 300).unwrap();
    assert_eq!(report.buckets, 0);
    assert_eq!(report.rows, 0);
}

#[test]
fn samples_of_two_packs_do_not_mix_in_a_bucket() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let first = register_pack(&ledger, "ABC123");
    let second = register_pack(&ledger, "ABC456");
    write(
        &ledger,
        &[
            sample(first, 0, 100.0, Some(40.0)),
            sample(second, 0, 900.0, Some(80.0)),
        ],
    );
    ledger.measurements().unwrap().rollup(0, 300).unwrap();
    let rows = ledger
        .measurements()
        .unwrap()
        .range(Tier::FiveMinute, second, 0, 300)
        .unwrap();
    assert_eq!(rows[0].power_w, Some(900.0));
}
