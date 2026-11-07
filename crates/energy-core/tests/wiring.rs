//! Wiring: measurements, the cell-voltage latch and relearning after a completed session.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty,
    clippy::manual_assert_eq,
    clippy::assertions_on_constants,
    clippy::cast_precision_loss
)]

mod support;

use std::sync::Arc;

use energy_battery::learning::LearningStore;
use energy_battery::protection::CellLatchStore;
use energy_core::safety::{Authority, Reason};
use energy_core::wiring::{LedgerLatchStore, LedgerLearningStore};
use energy_ledger::{Ledger, Sample, Tier};
use support::fakes::{NOW, pack, plan};
use support::harness;

/// One sample per pack per minute, and only once per minute.
#[tokio::test]
async fn measurements_are_written_once_per_minute() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let id = h
        .ledger
        .packs()
        .expect("packs")
        .by_sn("SN-A")
        .expect("read")
        .expect("pack row")
        .id;
    let rows = h
        .ledger
        .measurements()
        .expect("measurements")
        .range(Tier::OneMinute, id, NOW - 60, NOW + 60)
        .expect("range");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].soc_pct.is_some());

    h.engine.tick_once(NOW + 5).await.expect("tick");
    let rows = h
        .ledger
        .measurements()
        .expect("measurements")
        .range(Tier::OneMinute, id, NOW - 60, NOW + 60)
        .expect("range");
    assert_eq!(
        rows.len(),
        1,
        "a second tick in the same minute wrote a row"
    );
}

/// Below the cutoff the latch blocks discharge and forces a charge; a healthy reading clears it.
#[tokio::test]
async fn a_low_cell_latch_blocks_discharge_until_a_fresh_reading_clears_it() {
    let mut reading = pack("SN-A", 50.0);
    reading.min_cell_v = Some(2.90);
    let h = harness::build(Authority::Auto, vec![reading], plan(NOW, -0.8, 0.96));
    let store = LedgerLatchStore::new(Arc::clone(&h.ledger));
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");

    let latch = store
        .load("SN-A")
        .expect("latch read")
        .expect("latch saved");
    assert!(latch.blocks_discharge());
    assert!(latch.forces_charge());

    let tick = h.engine.tick_once(NOW + 5).await.expect("tick");
    let report = tick.report("SN-A").expect("report");
    assert!(
        matches!(
            report.reason,
            Reason::CellVoltageGuard | Reason::CellVoltageEmergencyCharge
        ),
        "reason {:?}",
        report.reason
    );
    assert!(
        h.sink.writes().iter().all(|watts| *watts > 0.0),
        "a latched pack was discharged: {:?}",
        h.sink.writes()
    );

    h.fleet.set(vec![pack("SN-A", 50.0)]);
    let tick = h.engine.tick_once(NOW + 10).await.expect("tick");
    assert!(
        !store
            .load("SN-A")
            .expect("read")
            .expect("latch")
            .blocks_discharge()
    );
    assert!((tick.report("SN-A").expect("report").applied_w + 800.0).abs() < 1.0);
}

/// A completed run is re-extracted and hands the learner a fresh profile.
#[tokio::test]
async fn a_completed_session_triggers_a_relearn() {
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, 0.0, 0.96),
    );
    let id = h
        .ledger
        .packs()
        .expect("packs")
        .by_sn("SN-A")
        .expect("read")
        .expect("pack row")
        .id;
    seed_charge_run(&h.ledger, id);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");

    let store = LedgerLearningStore::new(Arc::clone(&h.ledger));
    let profile = store
        .load("SN-A")
        .expect("profile read")
        .expect("profile saved");
    assert_eq!(profile.pack_id, "SN-A");

    let events = h
        .ledger
        .events()
        .expect("events")
        .range(NOW, NOW + 10)
        .expect("range");
    assert!(
        events.iter().any(|event| event.kind == "learning.applied"),
        "the relearn was not journalled: {events:?}"
    );
}

/// A hold-bounded 30-minute charge run ending five minutes ago.
fn seed_charge_run(ledger: &Ledger, pack_id: i64) {
    let start = NOW - 2400;
    let mut samples = Vec::new();
    for index in 0..3 {
        samples.push(Sample::new(pack_id, start + index * 60, 0.0, Some(30.0)));
    }
    for index in 0..30 {
        let soc = 30.0 + index as f64;
        samples.push(Sample::new(
            pack_id,
            start + (3 + index) * 60,
            600.0,
            Some(soc),
        ));
    }
    for index in 0..3 {
        samples.push(Sample::new(
            pack_id,
            start + (33 + index) * 60,
            0.0,
            Some(59.0),
        ));
    }
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&samples)
        .expect("seed samples");
}
