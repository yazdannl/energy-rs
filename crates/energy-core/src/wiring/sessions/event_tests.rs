#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::scheduled_tests::seed_cycle;
use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::BatteryError;
use energy_battery::learning::{LearningEvent, LearningProfile, LearningStore};
use energy_ledger::{Ledger, Pack};

#[test]
fn a_failed_event_write_is_replayed_once_on_the_next_pass() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = seed_pack(&ledger, "SN-EVENT-FAIL");
    let store = FailingPublish::new(LedgerLearningStore::new(Arc::clone(&ledger)), 1);
    let base = 1_700_000_000;
    seed_cycle(&ledger, id, base);
    let now = base + 6 * 3600;
    let policy = SessionPolicy::new(300, 24);

    assert!(run(&ledger, &store, now, &policy).is_err());
    let pending = store.pending_events().expect("pending");
    assert_eq!(
        pending.len(),
        1,
        "the profile commit must carry an outbox row"
    );
    assert_eq!(pending[0].pack_id, "SN-EVENT-FAIL");
    assert!(store.load("SN-EVENT-FAIL").expect("load").is_some());
    let summary = pending[0].summary.clone();
    assert_eq!(
        learning_events(&ledger).len(),
        0,
        "the failed write must not journal"
    );

    let reports = run(&ledger, &store, now + 60, &policy).expect("replay");
    assert!(
        reports.is_empty(),
        "the retry must not relearn a fresh profile"
    );
    assert_eq!(learning_events(&ledger).len(), 1);
    assert_eq!(learning_events(&ledger)[0].message, summary);
    assert_eq!(
        store.pending_events().expect("pending"),
        Vec::<LearningEvent>::new(),
        "a published event must leave the outbox"
    );

    run(&ledger, &store, now + 120, &policy).expect("later pass");
    assert_eq!(learning_events(&ledger).len(), 1, "no duplicate replay");
}

#[test]
fn a_later_pack_failure_keeps_the_earlier_event_and_replays_the_later_one() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let first = seed_pack(&ledger, "SN-FIRST-PACK");
    let second = seed_pack(&ledger, "SN-SECOND-PACK");
    let store = FailingPublish::new(LedgerLearningStore::new(Arc::clone(&ledger)), 2);
    let base = 1_700_000_000;
    seed_cycle(&ledger, first, base);
    seed_cycle(&ledger, second, base);
    let now = base + 6 * 3600;
    let policy = SessionPolicy::new(300, 24);

    assert!(run(&ledger, &store, now, &policy).is_err());
    let events = learning_events(&ledger);
    assert_eq!(events.len(), 1, "the first pack must not lose its event");
    assert!(events[0].message.contains("SN-FIRST-PACK"));
    let pending = store.pending_events().expect("pending");
    assert_eq!(
        pending.len(),
        1,
        "only the later pack is still owed an event"
    );
    assert_eq!(pending[0].pack_id, "SN-SECOND-PACK");

    run(&ledger, &store, now + 60, &policy).expect("replay");
    let events = learning_events(&ledger);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.message.contains("SN-FIRST-PACK"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.message.contains("SN-SECOND-PACK"))
            .count(),
        1
    );
    assert_eq!(
        store.pending_events().expect("pending"),
        Vec::<LearningEvent>::new()
    );
}

fn seed_pack(ledger: &Ledger, sn: &str) -> i64 {
    ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new(sn, 1.92, 0))
        .expect("upsert")
        .id
}

fn learning_events(ledger: &Ledger) -> Vec<energy_ledger::Event> {
    ledger
        .events()
        .expect("events")
        .recent(1000)
        .expect("read events")
        .into_iter()
        .filter(|event| event.kind == "learning.applied")
        .collect()
}

/// Fail the `nth` event publish, then behave like the normal ledger store.
struct FailingPublish {
    inner: LedgerLearningStore,
    fail_at: usize,
    calls: AtomicUsize,
}

impl FailingPublish {
    fn new(inner: LedgerLearningStore, fail_at: usize) -> Self {
        Self {
            inner,
            fail_at,
            calls: AtomicUsize::new(0),
        }
    }
}

impl LearningStore for FailingPublish {
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError> {
        self.inner.load(pack_id)
    }

    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError> {
        self.inner.save(profile)
    }

    fn save_with_event(
        &self,
        profile: &LearningProfile,
        event: LearningEvent,
    ) -> Result<LearningEvent, BatteryError> {
        self.inner.save_with_event(profile, event)
    }

    fn pending_events(&self) -> Result<Vec<LearningEvent>, BatteryError> {
        self.inner.pending_events()
    }

    fn publish_event(&self, event: &LearningEvent) -> Result<(), BatteryError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) + 1 == self.fail_at {
            return Err(BatteryError::Invalid {
                reason: "injected event write failure".to_owned(),
            });
        }
        self.inner.publish_event(event)
    }
}
