#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::{CapacityBounds, DEFAULT_ROUND_TRIP, LearningReport};
use energy_ledger::{Pack, Sample};

#[test]
fn an_unmeasured_round_trip_does_not_void_the_capacity_confidence() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-CONF", 1.92, 0))
        .expect("upsert")
        .id;
    let sessions = (0..3)
        .map(|index| SessionRecord {
            pack_id: "SN-CONF".to_owned(),
            direction: BatteryDirection::Charge,
            started_at: instant(index * 2 * 3600),
            ended_at: instant((index * 2 + 1) * 3600),
            soc_start_pct: 20.0,
            soc_end_pct: 50.0,
            energy_ac_kwh: 0.3 * 1.92 / 0.9,
            duration_s: 3600.0,
            sample_count: 60,
            complete: true,
        })
        .collect::<Vec<_>>();
    let report = LearningReport::learn(
        "SN-CONF",
        &sessions,
        &CapacityBounds::new(1.92, None),
        None,
        instant(8 * 3600),
    );
    assert!(report.capacity.confidence > 0.0);
    assert_eq!(report.efficiency.confidence, 0.0);
    store_row(&ledger, id, &report, 100).expect("store row");
    let row = ledger
        .packs()
        .expect("packs")
        .get(id)
        .expect("read")
        .expect("row");
    assert_eq!(row.learned_confidence, Some(report.capacity.confidence));
}

#[test]
fn a_short_discharge_barrier_survives_extraction_and_splits_charge_blocks() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-BARRIER", 1.92, 0))
        .expect("upsert")
        .id;
    let base = 1_700_000_000;
    seed_barrier_run(&ledger, id, base);
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let now = base + 66 * 60;
    let reports = relearn_now(
        &ledger,
        &store,
        now,
        &SessionPolicy::new(3600, 24),
        Some("SN-BARRIER"),
    )
    .expect("relearn");
    assert_eq!(reports.len(), 1);

    let sessions = ledger
        .sessions()
        .expect("sessions")
        .for_pack(id, base, now + 1)
        .expect("read");
    assert!(
        sessions.iter().any(|session| {
            session.direction == LedgerDirection::Discharge && session.sample_count == 1
        }),
        "the one-sample discharge was dropped: {sessions:?}"
    );
    let attempts = &reports[0].efficiency.diagnostics.attempts;
    let last = attempts.last().expect("pair attempt");
    assert_eq!(last.charge_segments, 1, "{attempts:?}");
    assert!((last.soc_from_pct - 40.0).abs() < 1e-9, "{attempts:?}");
}

#[test]
fn an_invalidated_charge_never_becomes_a_measured_round_trip() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-INVALID", 1.92, 0))
        .expect("upsert")
        .id;
    let base = 1_700_000_000;
    seed_invalid_charge_run(&ledger, id, base);
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let now = base + 53 * 60;
    let reports = relearn_now(
        &ledger,
        &store,
        now,
        &SessionPolicy::new(3600, 24),
        Some("SN-INVALID"),
    )
    .expect("relearn");
    assert_eq!(reports.len(), 1);
    let efficiency = &reports[0].efficiency;
    assert_eq!(efficiency.pairs, 0);
    assert_eq!(efficiency.round_trip, DEFAULT_ROUND_TRIP);
    let diagnostics = &efficiency.diagnostics;
    assert_eq!(diagnostics.candidates, 1);
    assert_eq!(diagnostics.reasons.get("session_incomplete"), Some(&1));
}

/// A plausible cycle whose charge leg lost one SoC reading during extraction.
fn seed_invalid_charge_run(ledger: &Ledger, pack_id: i64, base: i64) {
    let mut samples = Vec::new();
    let mut push = |minute: i64, power_w: f64, soc_pct: Option<f64>| {
        samples.push(Sample::new(pack_id, base + minute * 60, power_w, soc_pct));
    };
    push(0, 0.0, Some(20.0));
    for index in 0..20 {
        let soc = (index != 10).then_some(21.0 + index as f64);
        push(index + 1, 2000.0, soc);
    }
    push(21, 0.0, Some(50.0));
    for index in 0..30 {
        push(index + 22, -1200.0, Some(49.0 - index as f64));
    }
    push(52, 0.0, Some(19.0));
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed");
}

/// Charge, one-sample discharge, charge, discharge: all minute samples, hold-bounded runs.
fn seed_barrier_run(ledger: &Ledger, pack_id: i64, base: i64) {
    let mut samples = Vec::new();
    let mut push = |minute: i64, power_w: f64, soc_pct: f64| {
        samples.push(Sample::new(
            pack_id,
            base + minute * 60,
            power_w,
            Some(soc_pct),
        ));
    };
    push(0, 0.0, 20.0);
    for index in 0..20 {
        push(index + 1, 800.0, 21.0 + index as f64);
    }
    push(21, 0.0, 40.0);
    push(22, -800.0, 39.0);
    push(23, 0.0, 39.0);
    for index in 0..20 {
        push(index + 24, 800.0, 40.0 + index as f64);
    }
    push(44, 0.0, 59.0);
    for index in 0..20 {
        push(index + 45, -800.0, 58.0 - index as f64);
    }
    push(65, 0.0, 39.0);
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed");
}
