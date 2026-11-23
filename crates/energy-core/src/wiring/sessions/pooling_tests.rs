#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::scheduled_tests::{seed_cycle, seed_profile_with_clock};
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::CapacitySource;
use energy_ledger::Pack;

#[test]
fn skipped_peers_still_enter_the_capacity_pool() {
    let (ledger, store, now, policy, _) = seed_pool_fleet();
    let states = collect_states(&ledger, &store, now, &policy).expect("states");
    let pool = pool::fleet_pool(&states, now);
    assert_eq!(pool.packs_for_label(1.92), 3);

    let reports = run(&ledger, &store, now, &policy).expect("scheduled pass");
    assert_eq!(reports.len(), 1, "only the stale clock needs learning");
    assert_eq!(
        reports[0].capacity.source,
        CapacitySource::Pooled,
        "{:?}",
        reports[0]
    );
}

#[test]
fn a_targeted_relearn_keeps_the_whole_fleet_pool() {
    let (ledger, store, now, policy, _) = seed_pool_fleet();
    let states = collect_states(&ledger, &store, now, &policy).expect("states");
    assert_eq!(pool::fleet_pool(&states, now).packs_for_label(1.92), 3);

    let reports =
        relearn_now(&ledger, &store, now, &policy, Some("POOL-TARGET")).expect("targeted relearn");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].capacity.source, CapacitySource::Pooled);

    let states = collect_states(&ledger, &store, now, &policy).expect("states after");
    assert_eq!(
        pool::fleet_pool(&states, now).packs_for_label(1.92),
        3,
        "targeting a pack must not shrink its own pool"
    );
}

fn seed_pool_fleet() -> (Arc<Ledger>, LedgerLearningStore, i64, SessionPolicy, i64) {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let now = 1_700_000_000 + 6 * 24 * 3600;
    let base = now - 40 * 3600;
    for sn in ["POOL-A", "POOL-B", "POOL-TARGET"] {
        let id = ledger
            .packs()
            .expect("packs")
            .upsert(&Pack::new(sn, 1.92, 0))
            .expect("upsert")
            .id;
        for cycle in 0..3 {
            seed_cycle(&ledger, id, base + cycle * 12 * 3600);
        }
    }
    let learned_after_peers = base + 3 * 12 * 3600 + 60;
    for sn in ["POOL-A", "POOL-B"] {
        seed_profile_with_clock(&store, sn, learned_after_peers, now - 60, now - 60);
    }
    seed_profile_with_clock(
        &store,
        "POOL-TARGET",
        base - 60,
        now - 2 * 3600,
        now - 2 * 3600,
    );
    (ledger, store, now, SessionPolicy::new(300, 7 * 24), base)
}
