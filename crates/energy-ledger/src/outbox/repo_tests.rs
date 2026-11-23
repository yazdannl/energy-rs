#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs::remove_file;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::db::{Ledger, LedgerConfig};

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

fn shared_ledger(name: &str) -> (Arc<Ledger>, PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "energy-outbox-{name}-{}-{nanos}.db",
        std::process::id()
    ));
    remove_file(&path).ok();
    let ledger =
        Arc::new(Ledger::open(&LedgerConfig::at(&path).with_busy_timeout(5_000)).expect("ledger"));
    (ledger, path)
}

fn cleanup(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        remove_file(PathBuf::from(name)).ok();
    }
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

fn alert_events(ledger: &Ledger) -> Vec<crate::event::Event> {
    ledger
        .events()
        .expect("events")
        .recent(1000)
        .expect("read events")
        .into_iter()
        .filter(|event| event.kind == "learning.outbox_quarantined")
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
    assert_eq!(quarantined[0].summary.as_text(), Some("poison".as_bytes()));
    assert!(quarantined[0].reason.contains("poison payload"));
    assert!(
        alert_events(&ledger)[0]
            .message
            .contains(&format!("row {}", poison.id))
    );
    assert_eq!(learning_events(&ledger).len(), 1);
    assert_eq!(learning_events(&ledger)[0].message, "healthy");
}

#[test]
fn invalid_utf8_and_null_are_reconstructed_from_the_quarantine() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    ledger
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO learning_outbox (pack_id, at, summary)
             VALUES (CAST(X'FFFE' AS TEXT), 123, NULL)",
            [],
        )
        .expect("seed invalid utf8 and null");
    let (pending, quarantined) = {
        let outbox = ledger.learning_outbox().expect("outbox");
        (
            outbox.pending().expect("pending"),
            outbox.quarantined().expect("quarantine"),
        )
    };

    assert_eq!(pending, Vec::<PendingLearningEvent>::new());
    assert_eq!(quarantined.len(), 1);
    let row = &quarantined[0];
    assert_eq!(row.pack_id.storage_type, "text");
    assert_eq!(row.pack_id.as_text(), Some([0xFF, 0xFE].as_slice()));
    assert_eq!(row.at.storage_type, "integer");
    assert_eq!(row.at.bytes, b"123".to_vec());
    assert!(row.summary.is_null());
    assert_eq!(row.summary.as_text(), None);
    assert_eq!(row.summary.bytes, Vec::<u8>::new());
    assert_eq!(alert_events(&ledger).len(), 1);
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
        (
            outbox.pending().expect("pending"),
            outbox.quarantined().expect("quarantine"),
        )
    };
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, valid.id);
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        quarantined[0].summary.as_text(),
        Some("bad payload".as_bytes())
    );
    assert_eq!(quarantined[0].at.storage_type, "blob");
    assert_eq!(quarantined[0].at.bytes, vec![0]);
    assert_eq!(alert_events(&ledger).len(), 1);
}

#[test]
fn two_connections_racing_on_a_malformed_row_quarantine_once() {
    let (first, path) = shared_ledger("malformed-race");
    let second = Arc::new(
        Ledger::open(&LedgerConfig::at(&path).with_busy_timeout(5_000)).expect("second ledger"),
    );
    first
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO learning_outbox (pack_id, at, summary) VALUES ('SN-RACE', 100, NULL)",
            [],
        )
        .expect("seed malformed row");

    thread::scope(|scope| {
        for ledger in [&first, &second] {
            let ledger = Arc::clone(ledger);
            scope.spawn(move || {
                ledger
                    .learning_outbox()
                    .expect("outbox")
                    .pending()
                    .expect("pending");
            });
        }
    });

    let quarantined = first
        .learning_outbox()
        .expect("outbox")
        .quarantined()
        .expect("quarantine");
    assert_eq!(quarantined.len(), 1, "exactly one row is quarantined");
    let remaining: i64 = first
        .lock()
        .expect("connection")
        .query_row("SELECT COUNT(*) FROM learning_outbox", [], |row| row.get(0))
        .expect("count source rows");
    assert_eq!(remaining, 0);
    assert_eq!(alert_events(&first).len(), 1, "no duplicate alert");
    cleanup(&path);
}
