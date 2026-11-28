//! A real v14 → v15 upgrade: profiles written by `LedgerLearningStore`, migration run by `Ledger`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::control::Direction as BatteryDirection;
use energy_battery::learning::round_trip::{RoundTripSource, published_round_trip, resolve};
use energy_battery::learning::{
    CapacityBounds, DEFAULT_ROUND_TRIP, LearningProfile, LearningReport, NEVER_MEASURED_AT,
    SessionRecord,
};
use energy_ledger::{Ledger, Pack};

const MEASURED: &str = "SN-UPGRADE-MEASURED";
const SEED: &str = "SN-UPGRADE-SEED";

fn closed_cycles(sn: &str) -> Vec<SessionRecord> {
    (0..4)
        .flat_map(|day| {
            let charge = SessionRecord {
                pack_id: sn.to_owned(),
                direction: BatteryDirection::Charge,
                started_at: instant(day * 2 * 3600),
                ended_at: instant((day * 2 + 1) * 3600),
                soc_start_pct: 20.0,
                soc_end_pct: 100.0,
                energy_ac_kwh: 2.0,
                duration_s: 3600.0,
                sample_count: 60,
                complete: true,
            };
            let discharge = SessionRecord {
                pack_id: sn.to_owned(),
                direction: BatteryDirection::Discharge,
                started_at: instant((day * 2 + 1) * 3600),
                ended_at: instant((day * 2 + 2) * 3600),
                soc_start_pct: 100.0,
                soc_end_pct: 20.0,
                energy_ac_kwh: 1.66,
                duration_s: 3600.0,
                sample_count: 60,
                complete: true,
            };
            [charge, discharge]
        })
        .collect()
}

/// A confident measurement carried forward on thin evidence: still `efficiency_applied = true`.
fn retained_profile() -> LearningProfile {
    let bounds = CapacityBounds::new(1.92, None);
    let sessions = closed_cycles(MEASURED);
    let confident = LearningProfile::from_report(&LearningReport::learn(
        MEASURED,
        &sessions,
        &bounds,
        None,
        instant(10 * 3600),
    ));
    let thin = LearningReport::learn(
        MEASURED,
        &sessions[..2],
        &bounds,
        Some(&confident),
        instant(11 * 3600),
    );
    assert!(thin.efficiency.kept_previous, "the prior must be retained");
    LearningProfile::from_report(&thin)
}

fn unmeasured_profile() -> LearningProfile {
    let report = LearningReport::learn(
        SEED,
        &[],
        &CapacityBounds::new(1.92, None),
        None,
        instant(0),
    );
    assert!(
        !report.efficiency.applied,
        "a typed seed is not a measurement"
    );
    LearningProfile::from_report(&report)
}

/// Real store-written profiles plus the legacy CURRENT stamp on each pack row.
fn seed_legacy_file(path: &Path, measured: &LearningProfile, seed: &LearningProfile) {
    let ledger = Arc::new(Ledger::open_at(path).expect("ledger"));
    {
        let packs = ledger.packs().expect("packs");
        for (sn, round_trip) in [(MEASURED, measured.round_trip), (SEED, DEFAULT_ROUND_TRIP)] {
            let id = packs.upsert(&Pack::new(sn, 1.92, 100)).expect("pack").id;
            packs
                .set_learning(id, Some(1.80), round_trip, 0.8, 1_700_000_000)
                .expect("current legacy stamp");
        }
    }
    // The settings repository takes the ledger's connection mutex too: release `packs` first.
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    store.save(measured).expect("real measured profile");
    store.save(seed).expect("real unmeasured profile");
}

/// Roll back to v14; profile JSON and current stamps stay exactly as the old binary wrote them.
fn downgrade_to_v14(path: &Path) {
    let raw = rusqlite::Connection::open(path).expect("raw ledger");
    raw.execute_batch(
        "ALTER TABLE packs DROP COLUMN manual_round_trip;
         UPDATE meta SET value = '14' WHERE key = 'schema_version';",
    )
    .expect("v14 shape");
}

fn measured_row(ledger: &Ledger) -> energy_ledger::Pack {
    ledger
        .packs()
        .expect("packs")
        .by_sn(MEASURED)
        .expect("read")
        .expect("measured pack")
}

fn seed_row(ledger: &Ledger) -> energy_ledger::Pack {
    ledger
        .packs()
        .expect("packs")
        .by_sn(SEED)
        .expect("read")
        .expect("seed pack")
}

fn effective(pack: &energy_ledger::Pack) -> (f64, RoundTripSource) {
    resolve(
        published_round_trip(pack.learned_round_trip, pack.learned_at),
        pack.manual_round_trip,
    )
}

#[test]
fn a_real_v14_to_v15_upgrade_resolves_from_durable_profile_evidence() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("legacy.db");
    let measured = retained_profile();
    let seed = unmeasured_profile();
    seed_legacy_file(&path, &measured, &seed);
    downgrade_to_v14(&path);

    // This open performs the real migration and its backfill.
    let ledger = Ledger::open_at(&path).expect("upgraded ledger");
    let json: String = ledger
        .settings()
        .expect("settings")
        .get(&LedgerLearningStore::key(MEASURED))
        .expect("read profile")
        .expect("the real store wrote JSON");
    assert!(json.contains("\"efficiency_applied\":true"));

    let row = measured_row(&ledger);
    assert!(row.learned_at.is_some_and(|at| at > 0));
    assert_eq!(
        effective(&row),
        (measured.round_trip, RoundTripSource::Learned)
    );

    let row = seed_row(&ledger);
    assert_eq!(row.learned_at, Some(NEVER_MEASURED_AT.timestamp()));
    assert_eq!(row.manual_round_trip, None);
    assert_eq!(
        effective(&row),
        (DEFAULT_ROUND_TRIP, RoundTripSource::Default)
    );

    ledger
        .packs()
        .expect("packs")
        .set_manual_round_trip(SEED, Some(0.83))
        .expect("operator entry after upgrade");
    assert_eq!(
        effective(&seed_row(&ledger)),
        (0.83, RoundTripSource::Manual)
    );
}
