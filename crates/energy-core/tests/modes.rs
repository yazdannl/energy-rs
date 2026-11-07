//! The three control modes: what reaches the wire and what only reaches the journal.
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

/// `dry_run` runs the whole ladder, journals the intended write and sends nothing.
#[tokio::test]
async fn dry_run_applies_nothing_but_logs_the_intended_write() {
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let tick = h.engine.tick_once(NOW + 5).await.expect("tick");

    assert!(h.sink.commands().is_empty(), "dry run wrote something");
    let report = tick.report("SN-A").expect("pack report");
    assert!(!report.written);
    assert!((report.applied_w - 0.0).abs() < f64::EPSILON);
    assert!(
        (report.intended_w + 800.0).abs() < 1.0,
        "intended {}",
        report.intended_w
    );
    assert_eq!(report.reason, Reason::ModeNotAuto);
    assert!(h.engine.latch().expect("latch").is_empty());

    let rows = h
        .ledger
        .dispatch()
        .expect("dispatch")
        .range(NOW, NOW + 10)
        .expect("rows");
    assert!(
        rows.iter()
            .any(|row| row.reason == "mode_not_auto" && row.applied_w.abs() < f64::EPSILON),
        "the intended write is not in the dispatch log: {rows:?}"
    );
    assert!(rows.iter().all(|row| !row.verified));
}

/// `advisory` computes plans and never touches a device, not even to hold one.
#[tokio::test]
async fn advisory_never_holds_not_even_on_shutdown() {
    let h = harness::build(
        Authority::Advisory,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let tick = h.engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(
        tick.report("SN-A").expect("report").intended_w.round(),
        -800.0
    );
    h.engine.stop(NOW + 10, "sigterm").await.expect("stop");
    assert!(h.sink.commands().is_empty(), "advisory wrote something");
}

/// `auto` writes the setpoint and only reports it once the echo confirmed it.
#[tokio::test]
async fn auto_applies_the_setpoint_and_verifies_its_echo() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let tick = h.engine.tick_once(NOW + 5).await.expect("tick");

    assert_eq!(h.sink.writes(), vec![-800.0]);
    let report = tick.report("SN-A").expect("report");
    assert!(report.written && report.verified);
    assert!((report.applied_w + 800.0).abs() < 1.0);

    let latch = h.engine.latch().expect("latch");
    assert_eq!(latch.len(), 1, "a real write must latch");

    let rows = h
        .ledger
        .dispatch()
        .expect("dispatch")
        .range(NOW + 5, NOW + 6)
        .expect("rows");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].verified);
    assert!((rows[0].applied_w + 800.0).abs() < 1.0);
}

/// A failing tick is journalled, not guessed around: the plan is refused and the fleet holds.
#[tokio::test]
async fn a_refused_plan_holds_instead_of_keeping_the_last_setpoint() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.source.refuse(true);
    h.engine.boot(NOW).await.expect("boot");
    let tick = h.engine.tick_once(NOW).await.expect("tick");
    assert!(tick.stale_input);
    assert_eq!(
        tick.report("SN-A").expect("report").reason,
        Reason::StaleInput
    );
    let events = h
        .ledger
        .events()
        .expect("events")
        .range(NOW, NOW + 10)
        .expect("range");
    assert!(events.iter().any(|event| event.kind == "plan.refused"));
}
