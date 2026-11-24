//! The live-write latch: what survives a restart, and what may clear it.
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

use energy_battery::control::{ControlFault, Direction};
use energy_core::engine::LiveLatch;
use energy_core::safety::Authority;
use support::fakes::{self, NOW, pack, plan};
use support::harness;

/// A real setpoint latches, survives a restart and only a verified hold clears it.
#[tokio::test]
async fn the_latch_survives_a_restart_and_clears_only_on_a_verified_hold() {
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
    assert_eq!(LiveLatch::load(&ledger).expect("reload").len(), 1);

    let second = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW + 5, -0.8, 0.96),
    );
    second.sink.unverified_holds(true);
    second.engine.boot(NOW + 10).await.expect("boot");
    assert_eq!(
        second.sink.holds(),
        1,
        "boot must neutralise the latched pack"
    );
    assert_eq!(
        second.engine.latch().expect("latch").len(),
        1,
        "an unverified hold must not clear the latch"
    );
    assert_eq!(LiveLatch::load(&ledger).expect("reload").len(), 1);

    let third = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW + 10, -0.8, 0.96),
    );
    third.engine.boot(NOW + 20).await.expect("boot");
    assert_eq!(third.sink.holds(), 1);
    assert!(third.engine.latch().expect("latch").is_empty());
    assert!(LiveLatch::load(&ledger).expect("reload").is_empty());
}

/// A superseded write leaves the pre-write claim for the replacement hold and sends no second hold.
#[tokio::test]
async fn a_superseded_write_leaves_no_latch_and_sends_no_second_hold() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("prime the plan");
    h.sink.supersede_writes(true);
    let holds_before = h.sink.holds();
    let outcome = h.engine.tick_once(NOW + 5).await.expect("tick");
    let report = outcome.report("SN-A").expect("pack report");
    assert!(!report.written && !report.fail_safe && report.superseded.is_some());
    assert!(report.error.is_none());
    assert_eq!(h.sink.writes(), vec![-800.0]);
    assert_eq!(
        h.sink.holds(),
        holds_before,
        "commands={:?}",
        h.sink.commands()
    );
    // The pre-write claim stays put for the replacement hold; no extra latch row or churn.
    let latch = h.engine.latch().expect("latch");
    assert_eq!(latch.len(), 1);
    assert_eq!(latch.get("SN-A").expect("claimed").power_w, -800.0);
    assert_eq!(latch.get("SN-A").expect("claimed").since, NOW + 5);
    assert_eq!(LiveLatch::load(&h.ledger).expect("reload").len(), 1);
}

/// Boot writes nothing on hardware it never drove, and the latch stays empty.
#[tokio::test]
async fn boot_never_touches_a_device_it_did_not_drive() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0), pack("SN-B", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    assert!(
        h.sink.commands().is_empty(),
        "boot held a device with no latch: {:?}",
        h.sink.commands()
    );
    assert!(LiveLatch::load(&h.ledger).expect("latch").is_empty());
}

/// A blocked or shadowed tick never latches: only a real write does.
#[tokio::test]
async fn only_a_real_write_latches() {
    let h = harness::build(
        Authority::DryRun,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");
    assert!(h.sink.commands().is_empty());
    assert!(h.engine.latch().expect("latch").is_empty());
    assert!(LiveLatch::load(&h.ledger).expect("latch").is_empty());
}

/// An echo with a zero placeholder is not neutral flow: the accepted charge stays latched until a
/// real measurement proves it stopped.
#[tokio::test]
async fn an_echo_only_zero_never_clears_the_latch_of_a_nonzero_command() {
    let ledger = fakes::ledger_with(&["SN-A"]);
    let first = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, 0.8, 0.96),
    );
    first.sink.echo_only(true);
    first.engine.boot(NOW).await.expect("boot");
    first.engine.tick_once(NOW).await.expect("prime the plan");
    let outcome = first
        .engine
        .tick_once(NOW + 5)
        .await
        .expect("echo-only tick");
    let report = outcome.report("SN-A").expect("report");
    assert!(report.written && report.verified && !report.flow_measured);
    assert_eq!(report.applied_w, 0.0, "placeholder, not a measurement");
    assert!(report.applied_w_reason.is_some(), "an echo is not a sample");
    let latch = first.engine.latch().expect("latch");
    assert_eq!(latch.get("SN-A").expect("still latched").power_w, 800.0);
    assert_eq!(LiveLatch::load(&ledger).expect("reload").len(), 1);

    // Recovery: a restart neutralises the claim and the measured hold is allowed to clear it.
    let second = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW + 5, 0.8, 0.96),
    );
    second.engine.boot(NOW + 10).await.expect("recovery boot");
    assert_eq!(
        second.sink.holds(),
        1,
        "boot must neutralise the latched pack"
    );
    assert!(second.engine.latch().expect("latch").is_empty());
    assert!(LiveLatch::load(&ledger).expect("reload").is_empty());
}

#[tokio::test]
async fn unknown_flow_from_a_no_flow_verdict_keeps_the_accepted_write_latched() {
    let ledger = fakes::ledger_with(&["SN-A"]);
    let h = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, 0.8, 0.96),
    );
    h.sink.fault_with(
        "SN-A",
        ControlFault::NoFlow {
            sn: "SN-A".to_owned(),
            direction: Direction::Charge,
            echoed_w: 800,
            observed_w: 0,
            sample_at: NOW - 1,
            write_at: NOW,
        },
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("prime the plan");
    let outcome = h
        .engine
        .tick_once(NOW + 5)
        .await
        .expect("tick with an unusable verdict");
    let report = outcome.report("SN-A").expect("report");
    assert!(report.written, "the command was accepted");
    assert_eq!(
        report.reason,
        energy_core::safety::Reason::ConfirmationPending
    );
    assert_eq!(
        report.applied_w_reason,
        Some(energy_core::dispatch::AppliedPowerReason::ConfirmationPending)
    );
    let latch = h.engine.latch().expect("latch");
    assert_eq!(
        latch
            .get("SN-A")
            .expect("accepted charge stays latched")
            .power_w,
        800.0
    );
    assert_eq!(LiveLatch::load(&ledger).expect("reload").len(), 1);
}
