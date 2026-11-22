#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::scheduled_tests::{seed_cycle, seed_profile_with_attempt};
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_ledger::Pack;

#[test]
fn an_old_attempt_without_a_new_session_relearns_once() {
    let (ledger, store, policy, learned_at, now) = seed_unchanged_cycle("SN-OLD-ATTEMPT");
    seed_profile_with_attempt(&store, "SN-OLD-ATTEMPT", learned_at, now - 2 * 3600);

    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(reports.len(), 1, "an old attempt must trigger one backfill");
    let profile = store
        .load("SN-OLD-ATTEMPT")
        .expect("load")
        .expect("profile");
    assert_eq!(
        profile.efficiency_diagnostics.last_attempt_at,
        Some(instant(now))
    );
    assert_eq!(profile.updated_at, instant(now));
}

#[test]
fn a_recent_attempt_without_a_new_session_is_skipped() {
    let (ledger, store, policy, learned_at, now) = seed_unchanged_cycle("SN-RECENT-ATTEMPT");
    seed_profile_with_attempt(&store, "SN-RECENT-ATTEMPT", learned_at, now - 60);

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
        profile.efficiency_diagnostics.blocker.as_deref(),
        Some("no_new_completed_session")
    );
}

#[test]
fn two_checks_without_new_evidence_relearn_exactly_once() {
    let (ledger, store, policy, learned_at, now) = seed_unchanged_cycle("SN-TWO-CHECKS");
    seed_profile_with_attempt(&store, "SN-TWO-CHECKS", learned_at, now - 2 * 3600);

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
}

fn seed_unchanged_cycle(sn: &str) -> (Arc<Ledger>, LedgerLearningStore, SessionPolicy, i64, i64) {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new(sn, 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    seed_cycle(&ledger, id, base);
    let learned_at = base + 280 * 60;
    let policy = SessionPolicy::new(300, 24 * 3600);
    (ledger, store, policy, learned_at, base + 420 * 60)
}
