#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::scheduled_tests::{seed_cycle, seed_profile_with_clock};
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::BatteryError;
use energy_battery::learning::{LearningProfile, LearningStore, NEVER_MEASURED_AT};
use energy_ledger::Pack;

#[test]
fn a_failed_profile_commit_is_repaired_by_the_immediate_retry() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-PUBLISH-FAIL", 1.92, 0))
        .expect("upsert")
        .id;
    let inner = LedgerLearningStore::new(Arc::clone(&ledger));
    let store = FailingStore {
        inner,
        fail_next_save: AtomicBool::new(true),
    };
    let base = 1_700_000_000;
    seed_cycle(&ledger, id, base);
    seed_profile_with_clock(
        &store.inner,
        "SN-PUBLISH-FAIL",
        base - 60,
        base - 60,
        base - 60,
    );
    let now = base + 6 * 3600;
    let policy = SessionPolicy::new(300, 24);

    assert!(
        run(&ledger, &store, now, &policy).is_err(),
        "the profile commit must fail once"
    );
    let row = ledger
        .packs()
        .expect("packs")
        .get(id)
        .expect("read")
        .expect("pack row");
    // One partial pair publishes no round trip; the row write still landed.
    assert_eq!(row.learned_at, Some(NEVER_MEASURED_AT.timestamp()));
    let profile = store
        .load("SN-PUBLISH-FAIL")
        .expect("load")
        .expect("old profile");
    assert_eq!(profile.updated_at, instant(base - 60));
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(base - 60)),
        "the failed commit must not grant a skip"
    );

    let reports = run(&ledger, &store, now, &policy).expect("immediate retry");
    assert_eq!(reports.len(), 1, "retry before the backfill window");
    let profile = store
        .load("SN-PUBLISH-FAIL")
        .expect("load")
        .expect("committed profile");
    assert_eq!(profile.updated_at, instant(now));
    assert_eq!(
        profile.efficiency_diagnostics.last_learning_at,
        Some(instant(now))
    );
}

struct FailingStore {
    inner: LedgerLearningStore,
    fail_next_save: AtomicBool,
}

impl LearningStore for FailingStore {
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError> {
        self.inner.load(pack_id)
    }

    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError> {
        if self.fail_next_save.swap(false, Ordering::SeqCst) {
            return Err(BatteryError::Invalid {
                reason: "injected profile commit failure".to_owned(),
            });
        }
        self.inner.save(profile)
    }
}
