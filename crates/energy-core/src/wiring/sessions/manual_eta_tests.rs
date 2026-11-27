//! The real publication path decides whether a manual round trip is still the default.
//!
//! These tests do not write `learned_round_trip` or `learned_at` by hand (except for the legacy row,
//! where that is exactly the shape being emulated): they run [`relearn_now`], the same function the
//! engine tick calls, and read the pack row it publishes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::scheduled_tests::seed_cycle;
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::round_trip::{RoundTripSource, published_round_trip, resolve};
use energy_battery::learning::{DEFAULT_ROUND_TRIP, LearningReport, NEVER_MEASURED_AT};
use energy_ledger::{Pack, Sample};

const SN: &str = "SN-MANUAL-ETA";
const MANUAL: f64 = 0.90;

/// A registered pack with an operator entry, as the ledger sees it after a PATCH.
fn pack_with_manual() -> (Arc<Ledger>, i64) {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let packs = ledger.packs().expect("packs");
    let id = packs
        .upsert(&Pack::new(SN, 1.92, 0))
        .expect("registered")
        .id;
    packs
        .set_manual_round_trip(SN, Some(MANUAL))
        .expect("manual entry");
    drop(packs);
    (ledger, id)
}

fn row(ledger: &Ledger) -> energy_ledger::Pack {
    ledger
        .packs()
        .expect("packs")
        .by_sn(SN)
        .expect("read")
        .expect("the pack row")
}

fn effective(ledger: &Ledger) -> (f64, RoundTripSource) {
    let pack = row(ledger);
    resolve(
        published_round_trip(pack.learned_round_trip, pack.learned_at),
        pack.manual_round_trip,
    )
}

fn one_charge_run(ledger: &Ledger, id: i64, at: i64) {
    let samples = (0..30)
        .map(|minute| Sample::new(id, at + minute * 60, 900.0, Some(40.0)))
        .collect::<Vec<_>>();
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seeded");
}

fn one_charge_sweep(ledger: &Ledger, id: i64, at: i64) {
    let mut samples = vec![Sample::new(id, at, 0.0, Some(20.0))];
    samples.extend((1..=120).map(|minute| {
        Sample::new(
            id,
            at + minute * 60,
            1000.0,
            Some(20.0 + minute as f64 * 2.0 / 3.0),
        )
    }));
    samples.push(Sample::new(id, at + 121 * 60, 0.0, Some(100.0)));
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seeded charge sweep");
}

fn relearn(ledger: &Ledger, store: &LedgerLearningStore, now: i64) -> Vec<LearningReport> {
    relearn_now(
        ledger,
        store,
        now,
        &SessionPolicy::new(300, 7 * 24),
        Some(SN),
    )
    .expect("relearn through the engine's publication path")
}

#[test]
fn a_never_measured_seed_profile_does_not_outrank_the_manual_entry() {
    let (ledger, id) = pack_with_manual();
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    // One short charge: capacity cannot publish and no closed cycle exists, so the report's
    // measured_at stays at the never-measured sentinel. The real path writes that sentinel.
    one_charge_run(&ledger, id, base);
    let reports = relearn(&ledger, &store, base + 3600);
    assert_eq!(reports.len(), 1);
    assert!(!reports[0].measured());

    let pack = row(&ledger);
    assert_eq!(pack.learned_at, Some(NEVER_MEASURED_AT.timestamp()));
    assert_eq!(pack.learned_round_trip, Some(DEFAULT_ROUND_TRIP));
    assert_eq!(effective(&ledger), (MANUAL, RoundTripSource::Manual));
}

#[test]
fn a_capacity_only_pass_keeps_the_unmeasured_stamp_and_the_manual_entry() {
    let (ledger, id) = pack_with_manual();
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    // Three full charge runs and no discharges: capacity can publish, but no closed cycle
    // exists. The report's measured_at is current for the capacity; the row must still say that
    // no round trip was measured.
    for run in 0..3 {
        one_charge_sweep(&ledger, id, base + run * 6 * 3600);
    }
    let reports = relearn(&ledger, &store, base + 3 * 6 * 3600 + 3600);
    assert_eq!(reports.len(), 1);
    assert!(
        reports[0].capacity.decision.applied(),
        "the capacity should have published"
    );
    assert_eq!(reports[0].efficiency.pairs, 0);

    let pack = row(&ledger);
    assert_eq!(pack.learned_at, Some(NEVER_MEASURED_AT.timestamp()));
    assert_eq!(pack.learned_round_trip, Some(DEFAULT_ROUND_TRIP));
    assert_eq!(effective(&ledger), (MANUAL, RoundTripSource::Manual));
}

#[test]
fn a_fresh_measurement_from_the_wiring_outranks_the_manual_entry() {
    let (ledger, id) = pack_with_manual();
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    for cycle in 0..4 {
        seed_cycle(&ledger, id, base + cycle * 6 * 3600);
    }
    let now = base + 4 * 6 * 3600 + 3600;
    let reports = relearn(&ledger, &store, now);
    assert_eq!(reports.len(), 1);
    let measured = reports[0].efficiency.round_trip;
    assert!(
        reports[0].efficiency.applied,
        "the closed cycles should measure: {:?}",
        reports[0].efficiency.diagnostics
    );
    assert!((measured - MANUAL).abs() > 1e-6);

    let pack = row(&ledger);
    assert!(pack.learned_at.is_some_and(|at| at > 0));
    assert_eq!(pack.learned_round_trip, Some(measured));
    assert_eq!(effective(&ledger), (measured, RoundTripSource::Learned));
}

#[test]
fn a_retained_confident_measurement_still_outranks_the_manual_entry() {
    let (ledger, id) = pack_with_manual();
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    for cycle in 0..4 {
        seed_cycle(&ledger, id, base + cycle * 6 * 3600);
    }
    let now = base + 4 * 6 * 3600 + 3600;
    let reports = relearn(&ledger, &store, now);
    let measured = reports[0].efficiency.round_trip;
    assert!(reports[0].efficiency.is_confident());

    // A one-hour window leaves only this short run for the next pass: too little to measure
    // again, but enough for the wiring to republish the kept profile with a real stamp.
    one_charge_run(&ledger, id, now + 60);
    let later = relearn_now(
        &ledger,
        &store,
        now + 3600,
        &SessionPolicy::new(300, 1),
        Some(SN),
    )
    .expect("relearn with thin evidence");
    assert_eq!(later.len(), 1);
    assert!(later[0].efficiency.kept_previous);
    assert!(!later[0].efficiency.applied);

    let pack = row(&ledger);
    assert!(pack.learned_at.is_some_and(|at| at >= now));
    assert_eq!(pack.learned_round_trip, Some(measured));
    assert_eq!(effective(&ledger), (measured, RoundTripSource::Learned));
}

#[test]
fn a_pre_v15_legacy_row_keeps_its_published_round_trip_over_the_manual_entry() {
    let (ledger, id) = pack_with_manual();
    // Upgrade-era shape: a v14 file had learned_capacity_kwh, learned_round_trip,
    // learned_confidence and learned_at, but no manual_round_trip column at all. After migration
    // v15 that column is NULL and the old published numbers still decide the efficiency.
    ledger
        .packs()
        .expect("packs")
        .set_manual_round_trip(SN, None)
        .expect("a pre-v15 file has no manual column");
    ledger
        .packs()
        .expect("packs")
        .set_learning(id, Some(1.80), 0.88, 0.7, 1_700_000_000)
        .expect("the older binary's published numbers");
    assert_eq!(effective(&ledger), (0.88, RoundTripSource::Learned));
}
