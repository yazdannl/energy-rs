#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::{CapacityBounds, LearningProfile, LearningReport};
use energy_ledger::{Pack, Sample};

#[test]
fn scheduled_relearn_learns_a_cycle_that_finished_hours_ago() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-SCHED", 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    seed_profile(&store, "SN-SCHED", base - 24 * 3600);
    seed_cycle(&ledger, id, base);

    // The charge started five hours ago: a one-hour extraction window cannot see its start, and
    // its completion is far older than two five-minute check intervals.
    let now = base + 300 * 60;
    let policy = SessionPolicy::new(300, 24 * 3600);
    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(reports.len(), 1, "a completed cycle must be learned");

    let profile = store.load("SN-SCHED").expect("load").expect("profile");
    assert_eq!(profile.updated_at, instant(now));
    let diagnostics = profile.efficiency_diagnostics;
    assert_eq!(diagnostics.last_attempt_at, Some(instant(now)));
    assert!(diagnostics.sessions_in_window >= 2, "{diagnostics:?}");
    assert!(diagnostics.candidates > 0, "{diagnostics:?}");
    assert!(!diagnostics.attempts.is_empty(), "{diagnostics:?}");
}

#[test]
fn incomplete_only_sessions_report_a_named_blocker_without_moving_updated_at() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-INCOMPLETE", 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    let learned_at = base - 24 * 3600;
    seed_profile(&store, "SN-INCOMPLETE", learned_at);
    let samples = (0..30)
        .map(|minute| Sample::new(id, base + minute * 60, 900.0, Some(40.0)))
        .collect::<Vec<_>>();
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed");

    let now = base + 60 * 60;
    let reports =
        run(&ledger, &store, now, &SessionPolicy::new(300, 24 * 3600)).expect("scheduled relearn");
    assert_eq!(reports.len(), 0, "an open run is not a completed cycle");
    let profile = store.load("SN-INCOMPLETE").expect("load").expect("profile");
    assert_eq!(profile.updated_at, instant(learned_at));
    let diagnostics = profile.efficiency_diagnostics;
    assert_eq!(diagnostics.last_attempt_at, Some(instant(now)));
    assert!(diagnostics.sessions_in_window > 0);
    assert_eq!(diagnostics.sessions_complete, 0);
    assert_eq!(diagnostics.blocker.as_deref(), Some("no_completed_session"));
}

#[test]
fn an_empty_window_reports_no_sessions_in_window() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-EMPTY", 1.92, 0))
        .expect("upsert");
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    seed_profile(&store, "SN-EMPTY", base - 24 * 3600);

    let reports =
        run(&ledger, &store, base, &SessionPolicy::new(300, 24 * 3600)).expect("scheduled relearn");
    assert_eq!(reports.len(), 0);
    let profile = store.load("SN-EMPTY").expect("load").expect("profile");
    let diagnostics = profile.efficiency_diagnostics;
    assert_eq!(diagnostics.sessions_in_window, 0);
    assert_eq!(diagnostics.sessions_seen, 0);
    assert_eq!(
        diagnostics.blocker.as_deref(),
        Some("no_sessions_in_window")
    );
    assert_eq!(diagnostics.last_attempt_at, Some(instant(base)));
}

#[test]
fn a_24h_gap_with_a_real_cycle_moves_updated_at() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-GAP", 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    seed_profile(&store, "SN-GAP", base);
    seed_cycle(&ledger, id, base + 3600);

    let now = base + 25 * 3600;
    let policy = SessionPolicy::new(300, 48 * 3600);
    assert!(due(now, Some(base), policy.check_seconds()));
    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(reports.len(), 1, "a cycle during the gap must be learned");
    let profile = store.load("SN-GAP").expect("load").expect("profile");
    assert_eq!(profile.updated_at, instant(now));
}

fn seed_profile(store: &LedgerLearningStore, sn: &str, at: i64) {
    let report =
        LearningReport::learn(sn, &[], &CapacityBounds::new(1.92, None), None, instant(at));
    let mut profile = LearningProfile::from_report(&report);
    profile.efficiency_diagnostics.last_attempt_at = Some(instant(at));
    store.save(&profile).expect("seed profile");
}

/// A full night charge, a hold, an evening discharge and a hold, all in one-minute samples.
fn seed_cycle(ledger: &Ledger, pack_id: i64, base: i64) {
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
    for minute in 1..=120 {
        push(minute, 1000.0, 20.0 + minute as f64 * 2.0 / 3.0);
    }
    push(121, 0.0, 100.0);
    for minute in 122..=272 {
        push(minute, -800.0, 100.0 - (minute - 121) as f64 * 8.0 / 15.0);
    }
    push(273, 0.0, 20.0);
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed");
}
