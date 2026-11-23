#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::scheduled_tests::{seed_cycle, seed_profile_with_clock};
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_ledger::{Pack, Sample};

#[test]
fn an_old_learning_pass_without_new_evidence_backfills_without_claiming_a_measurement() {
    let (ledger, store, policy, learned_at, now) = seed_unmeasured_run("SN-OLD-LEARNING");
    seed_profile_with_clock(
        &store,
        "SN-OLD-LEARNING",
        learned_at,
        now - 2 * 3600,
        now - 2 * 3600,
    );

    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(
        reports.len(),
        1,
        "an old learning pass must trigger one backfill"
    );
    assert!(!reports[0].measured(), "no cycle was measured");
    let profile = store
        .load("SN-OLD-LEARNING")
        .expect("load")
        .expect("profile");
    assert_eq!(
        profile.updated_at,
        instant(learned_at),
        "no measurement landed"
    );
    assert_eq!(
        profile.efficiency_diagnostics.last_attempt_at,
        Some(instant(now))
    );
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(now))
    );
    assert_eq!(
        profile.efficiency_diagnostics.blocker.as_deref(),
        Some("no_measurement")
    );
}

#[test]
fn a_recent_attempt_without_a_new_session_is_skipped() {
    let (ledger, store, policy, learned_at, now) = seed_unchanged_cycle("SN-RECENT-ATTEMPT");
    seed_profile_with_clock(&store, "SN-RECENT-ATTEMPT", learned_at, now - 60, now - 60);

    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(reports.len(), 0, "a recent attempt must not relearn again");
    let profile = store
        .load("SN-RECENT-ATTEMPT")
        .expect("load")
        .expect("profile");
    assert_eq!(profile.updated_at, instant(learned_at));
    assert_eq!(
        profile.efficiency_diagnostics.last_attempt_at,
        Some(instant(now))
    );
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(now - 60)),
        "a skip is not a learning pass"
    );
    assert_eq!(
        profile.efficiency_diagnostics.blocker.as_deref(),
        Some("no_new_completed_session")
    );
}

#[test]
fn two_checks_without_new_evidence_relearn_exactly_once() {
    let (ledger, store, policy, learned_at, now) = seed_unchanged_cycle("SN-TWO-CHECKS");
    seed_profile_with_clock(
        &store,
        "SN-TWO-CHECKS",
        learned_at,
        now - 2 * 3600,
        now - 2 * 3600,
    );

    let first = run(&ledger, &store, now, &policy).expect("first check");
    let second = run(&ledger, &store, now + 60, &policy).expect("second check");
    assert_eq!(first.len() + second.len(), 1, "one backfill, then quiet");
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 0);
    let profile = store.load("SN-TWO-CHECKS").expect("load").expect("profile");
    assert_eq!(
        profile.efficiency_diagnostics.last_attempt_at,
        Some(instant(now + 60))
    );
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(now))
    );
}

#[test]
fn a_full_five_minute_cadence_backfills_once_per_hour_not_every_check() {
    let (ledger, store, policy, learned_at, start) = seed_unchanged_cycle("SN-FULL-CADENCE");
    seed_profile_with_clock(
        &store,
        "SN-FULL-CADENCE",
        learned_at,
        start - 2 * 3600,
        start - 2 * 3600,
    );

    let mut learning_checks = Vec::new();
    for check in 1..=25 {
        let now = start + check * policy.check_seconds();
        let reports = run(&ledger, &store, now, &policy).expect("scheduled check");
        assert!(reports.len() <= 1);
        if reports.first().is_some_and(LearningReport::measured) {
            learning_checks.push(check);
        }
    }
    let profile = store
        .load("SN-FULL-CADENCE")
        .expect("load")
        .expect("profile");
    assert_eq!(learning_checks, vec![1, 13, 25]);
    assert_eq!(
        profile.efficiency_diagnostics.last_attempt_at,
        Some(instant(start + 25 * policy.check_seconds()))
    );
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(start + 25 * policy.check_seconds()))
    );
}

/// One complete discharge with no charge before it: a real learning pass that measures nothing.
fn seed_unmeasured_run(sn: &str) -> (Arc<Ledger>, LedgerLearningStore, SessionPolicy, i64, i64) {
    let (ledger, store, policy, id, base) = seed_ledger(sn);
    let mut samples = vec![Sample::new(id, base - 60, 0.0, Some(100.0))];
    for minute in 0..30 {
        samples.push(Sample::new(
            id,
            base + minute * 60,
            -800.0,
            Some(100.0 - minute as f64),
        ));
    }
    samples.push(Sample::new(id, base + 30 * 60, 0.0, Some(70.0)));
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed discharge");
    (ledger, store, policy, base + 60 * 60, base + 5 * 3600)
}

fn seed_unchanged_cycle(sn: &str) -> (Arc<Ledger>, LedgerLearningStore, SessionPolicy, i64, i64) {
    let (ledger, store, policy, id, base) = seed_ledger(sn);
    seed_cycle(&ledger, id, base);
    (ledger, store, policy, base + 280 * 60, base + 420 * 60)
}

fn seed_ledger(sn: &str) -> (Arc<Ledger>, LedgerLearningStore, SessionPolicy, i64, i64) {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new(sn, 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    let policy = SessionPolicy::new(300, 24);
    (ledger, store, policy, id, base)
}
