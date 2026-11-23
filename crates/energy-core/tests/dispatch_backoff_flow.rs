//! A suppressed tick still reports what the pack is doing, if a fresh read can tell us.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::dispatch::{AppliedPowerReason, PackFlow, PackReading};
use energy_core::safety::Authority;

use support::fakes::{NOW, pack, plan};
use support::harness::{self, Harness};

/// Tick at which the pack is already backed off and the command is still live.
const BACKOFF_AT: i64 = NOW + 30;

/// Four ticks of a pack ignoring its discharge command arm the long backoff.
async fn backed_off() -> Harness {
    let h = harness::build(
        Authority::Auto,
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    for offset in 1..=4 {
        h.engine
            .tick_once(NOW + 5 * offset)
            .await
            .expect("ignored-command tick");
    }
    assert!(h.engine.dispatcher().faults().blocking("SN-A", BACKOFF_AT));
    h
}

fn pack_reporting(power_w: f64, stale: bool, available: bool) -> PackReading {
    let mut reading = pack("SN-A", 50.0);
    reading.power_w = power_w;
    let discharging = power_w < -5.0;
    reading.flow = Some(PackFlow {
        grid_input_w: 0,
        output_pack_w: 0,
        output_home_w: if discharging { -power_w as i64 } else { 0 },
        ac_status: Some(i32::from(discharging)),
        dc_status: Some(2 * i32::from(discharging)),
    });
    reading.stale = stale;
    reading.available = available;
    reading
}

fn row_at(h: &Harness, at: i64) -> energy_ledger::DispatchEntry {
    h.ledger
        .dispatch()
        .expect("dispatch log")
        .range(at, at + 1)
        .expect("rows")
        .into_iter()
        .find(|row| row.reason == "fault_backoff")
        .expect("a backoff row")
}

#[tokio::test]
async fn a_pack_delivering_through_backoff_reports_its_measured_watts() {
    let h = backed_off().await;
    h.fleet.set(vec![pack_reporting(-799.0, false, true)]);
    let before = h.sink.len();

    let outcome = h.engine.tick_once(BACKOFF_AT).await.expect("backoff tick");
    let report = outcome.report("SN-A").expect("pack row");
    assert_eq!(report.applied_w, -799.0);
    assert!(report.flow_measured, "the watts came from a fresh read");
    assert!(!report.written, "nothing was sent this tick");
    assert!(!report.verified, "no echo was seen this tick");
    assert_eq!(report.reason.as_str(), "fault_backoff");
    assert!(report.error.as_deref().is_some_and(|error| {
        error.contains("not sent")
            && error.contains("backing off")
            && error.contains("still standing")
            && error.contains("799 W")
    }));
    assert_eq!(h.sink.len(), before, "backoff must not write");

    let row = row_at(&h, BACKOFF_AT);
    assert_eq!(row.written, Some(false));
    assert!(row.flow_measured);
    assert_eq!(row.applied_w, -799.0);
}

#[tokio::test]
async fn a_pack_that_stopped_during_backoff_reports_zero_and_says_no_flow() {
    let h = backed_off().await;
    h.fleet.set(vec![pack_reporting(0.0, false, true)]);

    let outcome = h.engine.tick_once(BACKOFF_AT).await.expect("backoff tick");
    let report = outcome.report("SN-A").expect("pack row");
    assert_eq!(report.applied_w, 0.0);
    assert!(
        report.flow_measured,
        "zero really was measured, not invented"
    );
    assert!(!report.written);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("no flow measured")),
        "zero must be explained: {:?}",
        report.error
    );
}

#[tokio::test]
async fn a_stale_or_unavailable_pack_reports_unknown_flow_not_zero_delivery() {
    for (stale, available) in [(true, true), (false, false)] {
        let h = backed_off().await;
        h.fleet.set(vec![pack_reporting(-799.0, stale, available)]);

        let outcome = h.engine.tick_once(BACKOFF_AT).await.expect("backoff tick");
        let report = outcome.report("SN-A").expect("pack row");
        assert_eq!(report.applied_w, 0.0);
        assert!(!report.flow_measured);
        assert_eq!(
            report.applied_w_reason,
            Some(AppliedPowerReason::SampleStale)
        );
        assert!(!report.written);
        assert!(!report.verified);
        assert!(
            report.error.as_deref().is_some_and(|error| {
                error.contains("no fresh reading") && error.contains("flow unknown")
            }),
            "staleness must be named: {:?}",
            report.error
        );
        let row = h
            .ledger
            .dispatch()
            .expect("dispatch")
            .range(BACKOFF_AT, BACKOFF_AT + 1)
            .expect("rows")
            .into_iter()
            .find(|row| row.applied_w_reason.as_deref() == Some("sample_stale"))
            .expect("row naming the unknown sample");
        assert_eq!(row.applied_w, 0.0);
    }
}

#[tokio::test]
async fn a_suppressed_stop_tick_still_reports_continuing_discharge() {
    // No plan, no prior write: nothing in this tick claims a command is standing, yet the fresh
    // reading shows real output. The watts must be published without inventing a command.
    let h = harness::build(
        Authority::Auto,
        vec![pack_reporting(-799.0, false, true)],
        plan(NOW, 0.0, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine
        .dispatcher()
        .faults()
        .arm("SN-A", NOW, "scripted backoff");

    let outcome = h
        .engine
        .tick_once(NOW + 5)
        .await
        .expect("suppressed stop tick");
    let report = outcome.report("SN-A").expect("pack row");
    assert_eq!(report.applied_w, -799.0);
    assert!(report.flow_measured);
    assert!(!report.written);
    assert!(!report.verified);
    assert!(
        report.error.as_deref().is_some_and(|error| {
            error.contains("no command is standing")
                && error.contains("flow unexplained")
                && error.contains("799 W")
        }),
        "the row must name the unexplained flow: {:?}",
        report.error
    );
}
