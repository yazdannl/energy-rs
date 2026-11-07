//! The authority gate: nothing reaches a device outside `auto`, and a superseded command never does.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty,
    clippy::manual_assert_eq,
    clippy::assertions_on_constants,
    clippy::cast_possible_wrap
)]

mod support;

use std::sync::Arc;

use energy_core::dispatch::{BoxFuture, CommandSink, SinkError, WriteReceipt};
use energy_core::engine::{
    EngineError, GuardedSink, LIVE_LATCH_KEY, LiveLatch, WriteGate, WriteIntent,
};
use energy_core::safety::Authority;
use support::fakes::{self, FakeSink, NOW, pack, plan};
use support::harness;

/// A journal that records nothing; these tests are about the gate, not about ownership.
struct NoJournal;

impl WriteIntent for NoJournal {
    fn claim(&self, _pack_id: &str, _watts: f64, _now: i64) -> Result<(), SinkError> {
        Ok(())
    }
}

fn guarded(gate: &Arc<WriteGate>, inner: &Arc<FakeSink>, token: u64) -> GuardedSink {
    GuardedSink::new(
        Arc::clone(inner) as Arc<dyn CommandSink>,
        Arc::clone(gate),
        Arc::new(NoJournal),
        token,
        NOW,
    )
}

/// A command queued before an emergency hold carries the old generation and is refused.
#[tokio::test]
async fn a_hold_fences_a_command_queued_before_it() {
    let gate = Arc::new(WriteGate::new(Authority::Auto));
    let inner = Arc::new(FakeSink::new());
    let stale = guarded(&gate, &inner, gate.token());

    gate.fence();
    let error = stale.write("SN-A", -700.0).await.expect_err("fenced");
    assert!(matches!(error, SinkError::Fenced { .. }), "{error:?}");
    assert!(
        inner.commands().is_empty(),
        "a superseded command reached the transport: {:?}",
        inner.commands()
    );

    let current = guarded(&gate, &inner, gate.token());
    current.write("SN-A", -700.0).await.expect("admitted");
    assert_eq!(inner.writes(), vec![-700.0]);
}

/// `dry_run` cannot write, not even the neutral hold, and it cannot write a stale token either.
#[tokio::test]
async fn a_dry_run_gate_refuses_even_a_hold() {
    for mode in [Authority::DryRun, Authority::Advisory] {
        let gate = Arc::new(WriteGate::new(mode));
        let inner = Arc::new(FakeSink::new());
        let sink = guarded(&gate, &inner, gate.token());
        for error in [
            sink.hold("SN-A").await.expect_err("hold refused"),
            sink.write("SN-A", -700.0).await.expect_err("write refused"),
        ] {
            assert!(matches!(error, SinkError::Fenced { .. }), "{mode:?}");
        }
        assert!(inner.commands().is_empty(), "{mode:?} wrote something");
    }
}

/// A `dry_run` restart must not neutralise the setpoints a previous `auto` run left behind.
#[tokio::test]
async fn boot_and_shutdown_write_nothing_outside_auto() {
    let ledger = fakes::ledger_with(&["SN-A"]);
    let first = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    first.engine.boot(NOW).await.expect("boot");
    first.engine.tick_once(NOW).await.expect("first tick");
    first.engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(first.engine.latch().expect("latch").len(), 1);
    energy_core::mode::save(&ledger, Authority::DryRun, NOW + 6).expect("stored mode");

    let second = harness::build_on(
        Arc::clone(&ledger),
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW + 10, -0.8, 0.96),
    );
    second.engine.boot(NOW + 10).await.expect("boot");
    assert_eq!(second.engine.mode().expect("mode"), Authority::DryRun);
    assert!(
        second.sink.commands().is_empty(),
        "a dry_run boot wrote something: {:?}",
        second.sink.commands()
    );
    assert_eq!(
        second.engine.latch().expect("latch").len(),
        1,
        "the latched pack must stay latched until a hold is verified"
    );
    second.engine.stop(NOW + 20, "sigterm").await.expect("stop");
    assert!(
        second.sink.commands().is_empty(),
        "a dry_run shutdown wrote something: {:?}",
        second.sink.commands()
    );
    assert_eq!(second.engine.latch().expect("latch").len(), 1);
}

/// An unreadable latch is a recovery error: no silent empty list, and `auto` stays refused.
#[tokio::test]
async fn a_corrupt_latch_refuses_auto_and_keeps_the_raw_value() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    let corrupt = "{\"batteries\": [this is not json";
    h.ledger
        .settings()
        .expect("settings")
        .set(LIVE_LATCH_KEY, corrupt, NOW)
        .expect("seeded latch");
    energy_core::mode::save(&h.ledger, Authority::Auto, NOW).expect("stored mode");

    h.engine.boot(NOW).await.expect("boot degrades, never dies");
    assert_eq!(
        h.engine.mode().expect("mode"),
        Authority::DryRun,
        "auto must not run while the latch is unreadable"
    );
    assert!(h.sink.commands().is_empty());
    let error = h
        .engine
        .set_mode(Authority::Auto, NOW + 1)
        .await
        .expect_err("auto refused");
    assert!(
        matches!(error, EngineError::LatchCorrupt { .. }),
        "{error:?}"
    );
    assert!(
        LiveLatch::load(&h.ledger).is_err(),
        "the raw value must survive"
    );
    assert_eq!(
        h.ledger
            .settings()
            .expect("settings")
            .get(LIVE_LATCH_KEY)
            .expect("read")
            .as_deref(),
        Some(corrupt)
    );

    h.engine.reconcile_latch(NOW + 2).expect("reconcile");
    h.engine
        .set_mode(Authority::Auto, NOW + 3)
        .await
        .expect("auto after reconciliation");
    assert!(LiveLatch::load(&h.ledger).expect("reload").is_empty());
}

/// Progress is judged on a monotonic clock, so a backwards wall clock cannot hide a stall.
#[tokio::test]
async fn a_backward_clock_still_stalls() {
    let h = harness::build_config(
        energy_core::engine::EngineConfig {
            mode: Authority::Auto,
            tick_seconds: 1,
            ..energy_core::engine::EngineConfig::default()
        },
        fakes::ledger_with(&["SN-A"]),
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("tick");

    // A clock that jumped an hour back is not a reason to believe the loop is alive.
    assert!(!h.engine.supervise(NOW - 3600).await.expect("guard"));
    tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
    assert!(
        h.engine.supervise(NOW - 3600).await.expect("guard"),
        "the monotonic anchor must still detect a stall"
    );
    assert!(h.sink.holds() >= 1);
}

/// A hold is a write: outside `auto` the engine's own bookkeeping records nothing either.
#[tokio::test]
async fn a_dry_run_never_journals_a_hold_it_did_not_write() {
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("tick");
    h.engine.stop(NOW + 5, "sigterm").await.expect("stop");
    let rows = h
        .ledger
        .dispatch()
        .expect("dispatch")
        .range(NOW, NOW + 10)
        .expect("rows");
    assert!(
        rows.iter().all(|row| row.reason != "shutdown_hold"),
        "a hold that never happened was journalled: {rows:?}"
    );
}

/// The gated sink is the only path, so a transport that cannot be reached is never called.
#[tokio::test]
async fn the_gate_is_checked_before_the_intent_and_the_transport() {
    struct NeverCalled;
    impl CommandSink for NeverCalled {
        fn write(
            &self,
            _pack_id: &str,
            _watts: f64,
        ) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            panic!("the transport must not be reached");
        }
        fn hold(&self, _pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            panic!("the transport must not be reached");
        }
    }
    let gate = Arc::new(WriteGate::new(Authority::Advisory));
    let sink = GuardedSink::new(
        Arc::new(NeverCalled),
        Arc::clone(&gate),
        Arc::new(NoJournal),
        gate.token(),
        NOW,
    );
    assert!(sink.hold("SN-A").await.is_err());
    assert!(sink.write("SN-A", 700.0).await.is_err());
}
