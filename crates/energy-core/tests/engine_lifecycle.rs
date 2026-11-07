//! Watchdog, staleness and the shutdown hold.
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

use energy_core::safety::{Authority, Reason};
use support::fakes::{NOW, pack, plan};
use support::harness;

/// Three tick intervals without a progress mark means: hold everything, once.
#[tokio::test]
async fn the_watchdog_holds_three_ticks_after_the_last_progress() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");
    let before = h.sink.holds();

    let stalled = h.engine.supervise(NOW + 20).await.expect("guard");
    assert!(!stalled, "exactly three ticks is not yet stalled");
    assert_eq!(h.sink.holds(), before);

    let stalled = h.engine.supervise(NOW + 21).await.expect("guard");
    assert!(stalled);
    assert_eq!(
        h.sink.holds(),
        before + 1,
        "the watchdog did not hold the pack"
    );
    assert!(h.engine.latch().expect("latch").is_empty());

    let events = h
        .ledger
        .events()
        .expect("events")
        .range(NOW, NOW + 30)
        .expect("range");
    assert!(events.iter().any(|event| event.kind == "watchdog.hold"));
}

/// Outside `auto` the watchdog notices but writes nothing at all.
#[tokio::test]
async fn the_watchdog_writes_nothing_outside_auto() {
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let stalled = h.engine.supervise(NOW + 100).await.expect("guard");
    assert!(!stalled);
    assert!(h.sink.commands().is_empty());
}

/// A plan older than the stale bound is dropped, not replayed as the last setpoint.
#[tokio::test]
async fn a_stale_plan_holds_instead_of_repeating_the_last_setpoint() {
    let mut reading = pack("SN-A", 50.0);
    reading.power_w = -800.0;
    let stale_plan = plan(NOW - 3600, -0.8, 0.96);
    let h = harness::build(Authority::Auto, vec![reading], stale_plan);
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let tick = h.engine.tick_once(NOW + 5).await.expect("tick");

    assert!(tick.stale_input);
    assert!(tick.report("SN-A").expect("report").reason == Reason::StaleInput);
    assert!(h.sink.writes().is_empty(), "a stale plan wrote a setpoint");
    assert!(h.sink.holds() >= 2, "a stale plan must hold");
}

/// SIGTERM neutralises what `auto` left running; advisory writes nothing on the way out.
#[tokio::test]
async fn sigterm_holds_in_auto_and_never_in_dry_run() {
    let auto = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    auto.engine.boot(NOW).await.expect("boot");
    auto.engine.tick_once(NOW).await.expect("first tick");
    auto.engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(auto.engine.latch().expect("latch").len(), 1);
    let before = auto.sink.holds();
    auto.engine.stop(NOW + 10, "sigterm").await.expect("stop");
    assert_eq!(auto.sink.holds(), before + 1);
    assert!(auto.engine.latch().expect("latch").is_empty());

    let dry = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    dry.engine.boot(NOW).await.expect("boot");
    dry.engine.tick_once(NOW).await.expect("tick");
    dry.engine.stop(NOW + 10, "sigterm").await.expect("stop");
    assert!(dry.sink.commands().is_empty());
}
