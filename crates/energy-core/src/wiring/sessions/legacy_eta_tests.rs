//! Legacy upgrade rows: the durable profile, not the shared stamp, proves a measurement.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::round_trip::{RoundTripSource, published_round_trip, resolve};
use energy_battery::learning::{
    CapacityBounds, DEFAULT_ROUND_TRIP, LearningProfile, LearningReport, NEVER_MEASURED_AT,
};
use energy_ledger::{Ledger, Pack};

const SN: &str = "SN-MANUAL-ETA";
const MANUAL: f64 = 0.90;

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

fn row(ledger: &Ledger) -> Pack {
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

#[test]
fn a_backfilled_legacy_seed_row_leaves_the_manual_entry_effective() {
    let (ledger, id) = pack_with_manual();
    // Pre-v15 shape: capacity learning stamped a current learned_at while the round trip stayed
    // the typed seed and the durable profile said efficiency_applied=false. After the v15
    // backfill that stamp is the never-measured sentinel.
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let report = LearningReport::learn(SN, &[], &CapacityBounds::new(1.92, None), None, instant(0));
    let mut profile = LearningProfile::from_report(&report);
    profile.efficiency_applied = false;
    store.save(&profile).expect("unmeasured profile");
    let packs = ledger.packs().expect("packs");
    packs
        .set_learning(id, Some(1.80), DEFAULT_ROUND_TRIP, 0.8, 1_700_000_000)
        .expect("legacy capacity-only stamp");
    packs
        .set_learning(
            id,
            Some(1.80),
            DEFAULT_ROUND_TRIP,
            0.8,
            NEVER_MEASURED_AT.timestamp(),
        )
        .expect("the v15 backfill result");
    drop(packs);
    assert_eq!(effective(&ledger), (MANUAL, RoundTripSource::Manual));
}

#[test]
fn a_measured_legacy_row_keeps_the_learned_value_over_the_manual_entry() {
    let (ledger, id) = pack_with_manual();
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let report = LearningReport::learn(SN, &[], &CapacityBounds::new(1.92, None), None, instant(0));
    let mut profile = LearningProfile::from_report(&report);
    profile.efficiency_applied = true;
    store.save(&profile).expect("measured profile");
    ledger
        .packs()
        .expect("packs")
        .set_learning(id, Some(1.80), 0.88, 0.8, 1_700_000_000)
        .expect("published legacy measurement");
    // The backfill leaves this stamp alone because the profile proves the measurement.
    assert_eq!(effective(&ledger), (0.88, RoundTripSource::Learned));
}

#[test]
fn a_missing_or_corrupt_profile_is_treated_as_unmeasured() {
    let (ledger, id) = pack_with_manual();
    ledger
        .packs()
        .expect("packs")
        .set_learning(
            id,
            Some(1.80),
            DEFAULT_ROUND_TRIP,
            0.8,
            NEVER_MEASURED_AT.timestamp(),
        )
        .expect("backfilled row");
    // No profile at all, then an unreadable one: both are backfilled to the sentinel by v15.
    assert_eq!(effective(&ledger), (MANUAL, RoundTripSource::Manual));
    ledger
        .settings()
        .expect("settings")
        .set(
            &format!("learning.profile.{SN}"),
            "{not json",
            1_700_000_000,
        )
        .expect("corrupt profile");
    assert_eq!(effective(&ledger), (MANUAL, RoundTripSource::Manual));
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
