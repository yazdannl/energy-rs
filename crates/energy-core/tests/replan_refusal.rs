//! A refused replan holds; the previous plan is never dispatched in its place.
//!
//! `PLAN.md` §6: "Any mismatch ⇒ refuse the solve and hold (never 'use a slightly wrong plan')".
//! Before this test the engine kept the old plan in state and kept sending its setpoints, so a plan
//! source that started refusing left a stale plan running for hours.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

mod support;

use energy_core::safety::{Authority, Reason};
use support::fakes::{NOW, pack, plan};
use support::harness;

/// One tick at a slot boundary with a refusing source holds instead of repeating the old setpoint.
#[tokio::test]
async fn a_refused_replan_holds_instead_of_dispatching_the_old_plan() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    let live = h.engine.tick_once(NOW + 5).await.expect("tick");
    let report = live.report("SN-A").expect("pack report");
    assert!(
        (report.intended_w + 800.0).abs() < 1.0,
        "the live plan should discharge 800 W, got {}",
        report.intended_w
    );

    // The source starts refusing; the next slot boundary forces a replan that cannot succeed.
    h.source.refuse(true);
    let refused = h.engine.tick_once(NOW + 900).await.expect("tick");
    let report = refused.report("SN-A").expect("pack report");
    assert!(
        report.intended_w.abs() < f64::EPSILON,
        "a refused solve dispatched {} W from the old plan",
        report.intended_w
    );
    assert_eq!(report.reason, Reason::StaleInput);
}

/// The hold lasts as long as the refusal does, not just for one tick.
#[tokio::test]
async fn the_hold_repeats_while_the_source_keeps_refusing() {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("tick");

    h.source.refuse(true);
    for step in 1..=4i64 {
        let tick = h
            .engine
            .tick_once(NOW + 900 * step)
            .await
            .expect("a held tick is not an error");
        let report = tick.report("SN-A").expect("pack report");
        assert!(
            report.intended_w.abs() < f64::EPSILON,
            "tick {step} dispatched {} W",
            report.intended_w
        );
    }
}
