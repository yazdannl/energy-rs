#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::scheduled_tests::seed_profile_with_clock;
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::NEVER_MEASURED_AT;
use energy_ledger::{Pack, Sample};

#[test]
fn a_month_old_run_stays_outside_the_learning_window() {
    let (ledger, id, store, now, policy) = seed_boundary_pack("SN-MONTH-OLD");
    let old = now - 30 * 24 * 3600;
    let samples = (0..30)
        .map(|minute| Sample::new(id, old + minute * 60, 900.0, Some(30.0)))
        .collect::<Vec<_>>();
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed old run");

    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(reports.len(), 0);
    let from = now - policy.window_seconds();
    let sessions = ledger
        .sessions()
        .expect("sessions")
        .for_pack(id, from, now + 1)
        .expect("window");
    assert!(
        sessions.is_empty(),
        "a month-old run must not be resurrected"
    );
    let profile = store.load("SN-MONTH-OLD").expect("load").expect("profile");
    assert_eq!(profile.updated_at, NEVER_MEASURED_AT);
}

#[test]
fn a_session_crossing_the_extraction_floor_is_preserved_once() {
    let (ledger, id, store, now, policy) = seed_boundary_pack("SN-CROSS-FLOOR");
    let lead = now - 3 * 3600;
    let charge_start = lead + 60;
    let charge_minutes = (now - charge_start) / 60;
    let mut samples = vec![Sample::new(id, lead, 0.0, Some(20.0))];
    for minute in 0..charge_minutes {
        samples.push(Sample::new(
            id,
            charge_start + minute * 60,
            900.0,
            Some(20.0 + minute as f64 * 60.0 / charge_minutes as f64),
        ));
    }
    samples.push(Sample::new(id, now, 0.0, Some(80.0)));
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed crossing run");
    seed_profile_with_clock(&store, "SN-CROSS-FLOOR", lead - 60, now - 60, now - 60);

    run(&ledger, &store, now, &policy).expect("first pass");
    let first = window_sessions(&ledger, id, now, &policy);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].started_at, charge_start);
    assert!(first[0].complete);

    run(&ledger, &store, now + 60, &policy).expect("second pass");
    let second = window_sessions(&ledger, id, now, &policy);
    assert_eq!(second.len(), 1, "no duplicate after re-extraction");
    assert_eq!(second[0].started_at, first[0].started_at);
    assert_eq!(second[0].ended_at, first[0].ended_at);
}

#[test]
fn an_old_unresolved_run_is_revisited_with_a_recent_attempt_timestamp() {
    let (ledger, id, store, now, policy) = seed_boundary_pack("SN-OLD-UNRESOLVED");
    let start = now - 3 * 3600;
    let end = start + 60 * 60;
    let mut samples = vec![Sample::new(id, start - 60, 0.0, Some(20.0))];
    samples.extend(
        (0..60)
            .map(|minute| Sample::new(id, start + minute * 60, 900.0, Some(20.0 + minute as f64))),
    );
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed open run");
    ledger
        .sessions()
        .expect("sessions")
        .extract(id, Tier::OneMinute, start - 60, end, &policy.rules)
        .expect("seed incomplete row");
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&[Sample::new(id, end, 0.0, Some(80.0))])
        .expect("seed closing hold");

    let stored = ledger
        .sessions()
        .expect("sessions")
        .for_pack(id, start - 60, now + 1)
        .expect("stored");
    assert_eq!(stored.len(), 1);
    assert!(!stored[0].complete, "the run started incomplete");
    assert!(stored[0].ended_at < now - policy.extract_seconds());

    seed_profile_with_clock(
        &store,
        "SN-OLD-UNRESOLVED",
        start - 120,
        now - 60,
        now - 2 * 3600,
    );
    run(&ledger, &store, now, &policy).expect("scheduled relearn");

    let sessions = window_sessions(&ledger, id, now, &policy);
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].complete, "the closing hold must be recognised");
    assert_eq!(sessions[0].started_at, start);
    let profile = store
        .load("SN-OLD-UNRESOLVED")
        .expect("load")
        .expect("profile");
    assert_eq!(
        profile.efficiency_diagnostics.last_attempt_at,
        Some(instant(now))
    );
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(now))
    );
}

fn seed_boundary_pack(sn: &str) -> (Arc<Ledger>, i64, LedgerLearningStore, i64, SessionPolicy) {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new(sn, 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    (
        ledger,
        id,
        store,
        1_700_000_000,
        SessionPolicy::new(300, 24),
    )
}

fn window_sessions(ledger: &Ledger, id: i64, now: i64, policy: &SessionPolicy) -> Vec<Session> {
    ledger
        .sessions()
        .expect("sessions")
        .for_pack(id, now - policy.window_seconds(), now + 60)
        .expect("window")
}
