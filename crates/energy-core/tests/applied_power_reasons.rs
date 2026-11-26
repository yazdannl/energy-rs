//! Applied-power reasons name their own path; an echo is never promoted to a fresh measurement.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::sync::Arc;

use energy_battery::control::{ControlFault, Direction};
use energy_core::dispatch::{
    AppliedPowerReason, BoxFuture, CommandSink, Dispatcher, FleetView, MeterReading, Overrides,
    PackReport, TickInput, WriteReceipt,
};
use energy_core::engine::{Engine, EngineConfig};
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeFleet, FakeMeter, FakeSource, NOW, pack, plan};

#[derive(Clone, Copy)]
enum Reply {
    /// The write landed; flow will be resolved by a later read.
    Pending,
    /// A previous command still owns the device; nothing was written.
    Blocked,
    /// The device echoed the write, but no fresh sample proved the flow.
    EchoOnly,
    /// A fresh sample carried the flow.
    Measured,
}

struct ReplySink(Reply);

impl CommandSink for ReplySink {
    fn write(&self, _pack_id: &str, _watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        let receipt = match self.0 {
            Reply::Pending => WriteReceipt::pending(),
            Reply::Blocked => WriteReceipt::blocked_by_pending(),
            Reply::EchoOnly => WriteReceipt::verified(0.0),
            Reply::Measured => WriteReceipt::verified(800.0).with_measurement(NOW),
        };
        Box::pin(async move { Ok(receipt) })
    }

    fn hold(&self, _pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        Box::pin(async { Ok(WriteReceipt::verified(0.0).with_measurement(NOW)) })
    }
}

type SinkError = energy_core::dispatch::SinkError;

async fn row_for(reply: Reply) -> PackReport {
    let view = FleetView::new(vec![pack("SN-A", 50.0)]);
    let planned = plan(NOW, 0.8, 0.96);
    let meter = MeterReading::default();
    let overrides = Overrides::none();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&planned),
        fleet: &view,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let outcome = Dispatcher::new(SafetyLimits::default(), true, 10)
        .tick(&input, &ReplySink(reply))
        .await;
    outcome.report("SN-A").expect("a row for the pack").clone()
}

#[tokio::test]
async fn pending_blocked_stale_and_measured_rows_never_share_a_reason() {
    let pending = row_for(Reply::Pending).await;
    assert_eq!(pending.reason, Reason::ConfirmationPending);
    assert_eq!(
        pending.applied_w_reason,
        Some(AppliedPowerReason::ConfirmationPending)
    );
    assert!(!pending.flow_measured && !pending.verified);

    let blocked = row_for(Reply::Blocked).await;
    assert_eq!(blocked.reason, Reason::PreviousCommandPending);
    assert_eq!(
        blocked.applied_w_reason,
        Some(AppliedPowerReason::PreviousCommandPending)
    );
    assert!(!blocked.written && !blocked.flow_measured);

    let stale = row_for(Reply::EchoOnly).await;
    assert_eq!(
        stale.applied_w_reason,
        Some(AppliedPowerReason::SampleStale)
    );
    assert!(stale.verified, "the echo did arrive");
    assert!(!stale.flow_measured, "but the sample was not fresh");
    assert_eq!(stale.applied_w, 0.0);
    assert_eq!(stale.measured_at, None);

    let measured = row_for(Reply::Measured).await;
    assert!(measured.flow_measured);
    assert_eq!(measured.measured_at, Some(NOW));
    assert_eq!(measured.applied_w_reason, None);
    assert_eq!(measured.applied_w, 800.0);
}

/// The 07:01:52Z contradiction: `appliedW=0`, `sample_stale`, `verified`, `written`, `flowMeasured`.
/// A stale-but-echoed receipt is real; the ledger must not turn its echo into a measurement.
#[tokio::test]
async fn an_echo_without_a_fresh_sample_is_not_persisted_as_flow_measured() {
    let ledger = support::fakes::ledger_with(&["SN-A"]);
    let engine = Engine::new(
        EngineConfig {
            mode: Authority::Auto,
            ..EngineConfig::default()
        },
        Arc::new(FakeFleet::one(50.0)),
        Arc::new(FakeMeter::default()),
        Arc::new(FakeSource::new(plan(NOW, 0.8, 0.96))),
        Arc::new(ReplySink(Reply::EchoOnly)),
        Arc::clone(&ledger),
    );
    engine.boot(NOW).await.expect("boot");
    engine.tick_once(NOW).await.expect("first tick");
    engine.tick_once(NOW + 5).await.expect("executing tick");

    let rows = ledger
        .dispatch()
        .expect("dispatch log")
        .range(NOW + 5, NOW + 6)
        .expect("rows");
    let row = rows
        .iter()
        .find(|row| row.verified && row.requested_w > 0.0)
        .unwrap_or_else(|| panic!("no echoed charge row in {rows:#?}"));
    assert_eq!(row.applied_w(), None);
    assert_eq!(row.applied_w_reason(), Some("sample_stale"));
    assert!(row.verified);
    assert!(
        !row.flow_measured(),
        "an echo cannot claim measured flow: {row:?}"
    );
}

fn no_flow(sample_at: i64, write_at: i64) -> ControlFault {
    ControlFault::NoFlow {
        sn: "SN-A".to_owned(),
        direction: Direction::Charge,
        echoed_w: 1000,
        observed_w: 0,
        sample_at,
        write_at,
    }
}

async fn tick_with_sink_at(sink: &support::fakes::FakeSink, now: i64) -> PackReport {
    let view = FleetView::new(vec![pack("SN-A", 50.0)]);
    let planned = plan(NOW, 1.0, 1.0);
    let meter = MeterReading::default();
    let overrides = Overrides::none();
    let input = TickInput {
        now,
        mode: Authority::Auto,
        plan: Some(&planned),
        fleet: &view,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    Dispatcher::new(SafetyLimits::default(), true, 10)
        .tick(&input, sink)
        .await
        .report("SN-A")
        .expect("row")
        .clone()
}

#[tokio::test]
async fn a_pre_write_idle_sample_is_pending_not_a_hold() {
    let sink = support::fakes::FakeSink::new();
    sink.fault_with("SN-A", no_flow(NOW - 10, NOW));
    let row = tick_with_sink_at(&sink, NOW).await;
    assert_eq!(row.reason, Reason::ConfirmationPending);
    assert_eq!(
        row.applied_w_reason,
        Some(AppliedPowerReason::ConfirmationPending)
    );
    assert_eq!(
        sink.holds(),
        0,
        "absence of post-write evidence cannot hold a charge"
    );
}

#[tokio::test]
async fn a_genuine_zero_after_the_flow_window_holds_and_names_both_clocks() {
    let sink = support::fakes::FakeSink::new();
    sink.fault_with("SN-A", no_flow(NOW + 46, NOW));
    let row = tick_with_sink_at(&sink, NOW).await;
    assert!(row.fail_safe);
    assert_eq!(sink.holds(), 1);
    assert_eq!(row.judged_sample_at, Some(NOW + 46));
    assert_eq!(row.judged_write_at, Some(NOW));
    assert_ne!(row.reason, Reason::ConfirmationPending);
}

#[tokio::test]
async fn a_post_write_sample_with_real_flow_confirms_without_a_hold() {
    let sink = support::fakes::FakeSink::new();
    sink.curtail(650);
    let row = tick_with_sink_at(&sink, NOW).await;
    assert!(row.flow_measured);
    assert_eq!(row.applied_w, 650.0);
    assert_ne!(row.reason, Reason::ConfirmationPending);
    assert_eq!(sink.holds(), 0);
}

async fn tick_with_sink(sink: &support::fakes::FakeSink) -> PackReport {
    tick_with_sink_at(sink, NOW).await
}

#[tokio::test]
async fn delayed_sample_clocks_do_not_reset_the_recovery_bound() {
    let sink = support::fakes::FakeSink::new();
    for now in [NOW, NOW + 10, NOW + 44] {
        sink.fault_with("SN-A", no_flow(NOW - 10, NOW));
        let row = tick_with_sink_at(&sink, now).await;
        assert_eq!(row.reason, Reason::ConfirmationPending);
        assert_eq!(sink.holds(), 0, "no hold before the original 45 s bound");
    }
    sink.fault_with("SN-A", no_flow(NOW - 10, NOW));
    let row = tick_with_sink_at(&sink, NOW + 46).await;
    assert!(row.fail_safe, "the bound escalates to a verified hold");
    assert_eq!(sink.holds(), 1);
    assert_eq!(
        row.judged_write_at,
        Some(NOW),
        "the write clock never resets"
    );
}

#[tokio::test]
async fn a_failed_hold_still_carries_the_verdict_clocks() {
    let sink = support::fakes::FakeSink::new();
    sink.fault_with("SN-A", no_flow(NOW + 46, NOW));
    sink.fail_hold(
        "SN-A",
        SinkError::ReadUnverified {
            pack_id: "SN-A".to_owned(),
            reason: "device silent".to_owned(),
        },
    );
    let row = tick_with_sink(&sink).await;
    assert_eq!(row.judged_sample_at, Some(NOW + 46));
    assert_eq!(row.judged_write_at, Some(NOW));
}
