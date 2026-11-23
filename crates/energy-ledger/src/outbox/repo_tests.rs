#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::thread;

use super::*;
use crate::db::Ledger;

fn seed(ledger: &Ledger, sn: &str, at: i64, summary: &str) -> PendingLearningEvent {
    let id = ledger
        .settings()
        .expect("settings")
        .set_with_learning_event(
            &format!("learning.profile.{sn}"),
            "{}",
            at,
            &PendingLearningEvent::new(0, sn.to_owned(), at, summary.to_owned()),
        )
        .expect("queue event");
    PendingLearningEvent::new(id, sn.to_owned(), at, summary.to_owned())
}

fn learning_events(ledger: &Ledger) -> Vec<crate::event::Event> {
    ledger
        .events()
        .expect("events")
        .recent(1000)
        .expect("read events")
        .into_iter()
        .filter(|event| event.kind == "learning.applied")
        .collect()
}

#[test]
fn publishing_the_same_id_twice_writes_one_event() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let event = seed(&ledger, "SN-IDEMPOTENT", 100, "one learning summary");
    {
        let outbox = ledger.learning_outbox().expect("outbox");
        assert_eq!(
            outbox.publish(&event).expect("first publish"),
            PublishOutcome::Published
        );
        assert_eq!(
            outbox.publish(&event).expect("stale publish"),
            PublishOutcome::AlreadyClaimed
        );
    }
    assert_eq!(learning_events(&ledger).len(), 1);
    assert_eq!(
        ledger
            .learning_outbox()
            .expect("outbox")
            .quarantined()
            .expect("quarantine"),
        Vec::<QuarantinedEvent>::new()
    );
}

#[test]
fn two_publishers_claim_the_same_row_once() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let event = seed(&ledger, "SN-COMPETING", 100, "one raced summary");
    let outcomes = thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let ledger = Arc::clone(&ledger);
                let event = event.clone();
                scope.spawn(move || {
                    ledger
                        .learning_outbox()
                        .expect("outbox")
                        .publish(&event)
                        .expect("publish")
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("publisher thread"))
            .collect::<Vec<_>>()
    });

    assert_eq!(
        outcomes
            .into_iter()
            .filter(|outcome| *outcome == PublishOutcome::Published)
            .count(),
        1
    );
    assert_eq!(learning_events(&ledger).len(), 1);
}

#[test]
fn a_poison_row_is_quarantined_and_does_not_block_the_next_row() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let poison = seed(&ledger, "SN-POISON", 100, "poison");
    let healthy = seed(&ledger, "SN-HEALTHY", 101, "healthy");
    ledger
        .lock()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER reject_poison BEFORE INSERT ON events
             WHEN NEW.message = 'poison'
             BEGIN SELECT RAISE(ABORT, 'poison payload'); END;",
        )
        .expect("trigger");
    let quarantined = {
        let outbox = ledger.learning_outbox().expect("outbox");
        assert_eq!(
            outbox.publish(&poison).expect("poison publish"),
            PublishOutcome::Quarantined
        );
        assert_eq!(
            outbox.publish(&healthy).expect("healthy publish"),
            PublishOutcome::Published
        );
        outbox.quarantined().expect("quarantine")
    };
    assert_eq!(quarantined.len(), 1);
    assert_eq!(quarantined[0].source_id, poison.id);
    assert_eq!(quarantined[0].summary, "poison");
    assert!(quarantined[0].reason.contains("poison payload"));
    let events = ledger.events().expect("events").recent(1000).expect("read");
    assert!(events.iter().any(|event| {
        event.kind == "learning.outbox_quarantined"
            && event.message.contains(&format!("row {}", poison.id))
    }));
    assert_eq!(learning_events(&ledger).len(), 1);
    assert_eq!(learning_events(&ledger)[0].message, "healthy");
}

#[test]
fn a_malformed_row_is_quarantined_and_valid_rows_still_replay() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let valid = seed(&ledger, "SN-VALID", 100, "valid payload");
    ledger
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO learning_outbox (pack_id, at, summary) VALUES ('SN-BAD', x'00', 'bad payload')",
            [],
        )
        .expect("seed malformed row");
    let (pending, quarantined) = {
        let outbox = ledger.learning_outbox().expect("outbox");
        let pending = outbox.pending().expect("pending");
        (pending, outbox.quarantined().expect("quarantine"))
    };
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, valid.id);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(quarantined[0].raw_summary, "bad payload");
    assert!(quarantined[0].raw_at.contains("blob[1]:00"));
    let events = ledger.events().expect("events").recent(1000).expect("read");
    assert!(
        events
            .iter()
            .any(|event| event.kind == "learning.outbox_quarantined")
    );
}
