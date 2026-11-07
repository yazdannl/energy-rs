//! WAL behaviour: a second connection reads while the first writes, without "database is locked".

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use std::sync::Arc;
use std::thread;

use energy_ledger::{Ledger, Tier};
use support::{TempDatabase, register_pack, sample};

const SAMPLES: i64 = 200;

#[test]
fn a_reader_and_a_writer_share_the_file_under_wal() {
    let db = TempDatabase::new();
    let writer = Arc::new(db.ledger());
    let reader = Arc::new(db.ledger());
    let pack = register_pack(&writer, "ABC123");

    let reader_thread = {
        let reader = Arc::clone(&reader);
        thread::spawn(move || {
            let mut observed_max = 0_usize;
            for _ in 0..200 {
                let measurements = reader.measurements().expect("measurements");
                let rows = measurements
                    .range(Tier::OneMinute, pack, 0, SAMPLES * 60 + 60)
                    .expect("the read succeeds under WAL");
                observed_max = observed_max.max(rows.len());
            }
            observed_max
        })
    };

    let writer_thread = {
        let writer = Arc::clone(&writer);
        thread::spawn(move || {
            let mut written = 0_usize;
            for batch in 0..20 {
                let samples: Vec<_> = (0..10)
                    .map(|step| {
                        let offset = i64::from(batch * 10 + step);
                        sample(pack, offset * 60, 1_000.0, Some(50.0))
                    })
                    .collect();
                written += writer
                    .measurements()
                    .expect("measurements")
                    .write_many(&samples)
                    .expect("the write succeeds under WAL");
            }
            written
        })
    };

    let observed_max = reader_thread.join().expect("the reader finishes");
    let written = writer_thread.join().expect("the writer finishes");
    assert_eq!(written, usize::try_from(SAMPLES).unwrap());
    assert!(observed_max <= usize::try_from(SAMPLES).unwrap());
    let rows = writer
        .measurements()
        .unwrap()
        .range(Tier::OneMinute, pack, 0, SAMPLES * 60 + 60)
        .unwrap();
    assert_eq!(rows.len(), usize::try_from(SAMPLES).unwrap());
}

#[test]
fn a_second_handle_sees_the_first_handles_commit() {
    let db = TempDatabase::new();
    let first = db.ledger();
    let second = db.ledger();
    let pack = register_pack(&first, "ABC123");
    first
        .measurements()
        .unwrap()
        .write(&sample(pack, 60, 100.0, Some(40.0)))
        .unwrap();
    let rows = second
        .measurements()
        .unwrap()
        .range(Tier::OneMinute, pack, 0, 120)
        .unwrap();
    assert_eq!(rows.len(), 1);
    let _: &Ledger = &second;
}
