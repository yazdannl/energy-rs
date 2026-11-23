//! Caller-level coverage: the dispatcher's real battery sink, pacing and fail-safe hold.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use energy_battery::{
    ControlLimits, DeviceController, FakeBehaviour, FakeDevice, FakeState, HttpTransport,
};
use energy_core::dispatch::{
    BatterySink, CommandSink, Dispatcher, FleetView, MeterReading, Overrides, Progress, TickInput,
    WriteReceipt,
};
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{NOW, pack, plan};

const SN: &str = "SN-A";

struct Marks(AtomicUsize);

impl Progress for Marks {
    fn mark(&self, _now: i64) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// A sink that reports a safety hold took over before the original write could be resolved.
#[derive(Default)]
struct SupersedingSink(Mutex<Vec<&'static str>>);

impl SupersedingSink {
    fn calls(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().clone()
    }
}

impl CommandSink for SupersedingSink {
    fn write(
        &self,
        _pack_id: &str,
        _watts: f64,
    ) -> energy_core::dispatch::BoxFuture<'_, Result<WriteReceipt, energy_core::dispatch::SinkError>>
    {
        self.0.lock().unwrap().push("write");
        Box::pin(async { Ok(WriteReceipt::superseded()) })
    }

    fn hold(
        &self,
        _pack_id: &str,
    ) -> energy_core::dispatch::BoxFuture<'_, Result<WriteReceipt, energy_core::dispatch::SinkError>>
    {
        self.0.lock().unwrap().push("hold");
        Box::pin(async { Ok(WriteReceipt::verified(0.0).with_measurement(NOW)) })
    }
}

async fn fixture(
    behaviour: FakeBehaviour,
    window: Duration,
) -> (
    FakeDevice,
    Arc<BatterySink<HttpTransport>>,
    Arc<DeviceController<HttpTransport>>,
) {
    let fake = FakeDevice::spawn(FakeState::new(SN).with_behaviour(behaviour))
        .await
        .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(2)).unwrap();
    let controller = Arc::new(DeviceController::new(transport, ControlLimits::unlimited()));
    controller.poll().await.unwrap();
    let sink = Arc::new(
        BatterySink::new(vec![Arc::clone(&controller)], Duration::MAX).with_confirm_window(window),
    );
    (fake, sink, controller)
}

#[tokio::test]
async fn an_unconfirmed_dispatch_records_pending_and_marks_progress() {
    let (fake, sink, _) = fixture(
        FakeBehaviour::default()
            .without_spacing_rule()
            .ignoring_writes(),
        Duration::from_millis(100),
    )
    .await;
    let marks = Marks(AtomicUsize::new(0));
    let plan = plan(NOW, -0.8, 0.96);
    let fleet = FleetView::new(vec![pack(SN, 50.0)]);
    let meter = MeterReading::default();
    let overrides = Overrides::none();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: Some(&marks),
        selection: None,
    };
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let outcome = dispatcher.tick(&input, sink.as_ref()).await;

    let report = outcome.report(SN).expect("pack report");
    assert_eq!(report.reason, Reason::ConfirmationPending);
    assert!(!report.fail_safe && !report.verified && !report.flow_measured);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("confirmation pending")),
        "{:?}",
        report.error
    );
    let state = fake.state().unwrap();
    assert_eq!(
        state.write_count(),
        1,
        "one write, never a retry inside the tick"
    );
    assert_eq!(state.writes[0]["properties"]["outputLimit"], 800);
    assert!(
        marks.0.load(Ordering::Relaxed) > 0,
        "the wait must mark progress"
    );
    fake.shutdown().await;
}

#[tokio::test]
async fn a_hold_after_a_pending_write_is_not_blocked() {
    let (fake, sink, controller) = fixture(
        FakeBehaviour::default()
            .without_spacing_rule()
            .ignoring_writes(),
        Duration::from_secs(2),
    )
    .await;
    let pending = sink.write(SN, -800.0).await.expect("write accepted");
    assert!(pending.confirmation_pending);
    assert!(
        controller.pending().is_some(),
        "the echo is resolved by a later read"
    );
    let held = sink.hold(SN).await.expect("hold succeeds");
    assert!(held.verified);
    assert_eq!(
        fake.state().unwrap().write_count(),
        2,
        "the hold was sent once, not stormed"
    );
    fake.shutdown().await;
}

#[tokio::test]
async fn a_superseded_dispatch_does_not_add_a_second_hold() {
    let sink = SupersedingSink::default();
    let plan = plan(NOW, -0.8, 0.96);
    let fleet = FleetView::new(vec![pack(SN, 50.0)]);
    let meter = MeterReading::default();
    let overrides = Overrides::none();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let outcome = dispatcher.tick(&input, &sink).await;

    let report = outcome.report(SN).expect("pack report");
    assert!(!report.fail_safe && report.error.is_none());
    assert!(!report.written && !report.verified);
    assert_eq!(sink.calls(), vec!["write"], "no second hold was sent");
}

/// Direct sink callers (boot/shutdown holds, maintenance) cannot hang: the bounded read inside the
/// end-to-end budget ends the attempt and leaves the accepted command pending for readback.
#[tokio::test(start_paused = true)]
async fn a_direct_hold_with_a_hung_read_is_bounded_and_stays_pending() {
    use energy_core::dispatch::SinkError;

    let fake = FakeDevice::spawn(
        FakeState::new(SN).with_behaviour(FakeBehaviour::default().without_spacing_rule()),
    )
    .await
    .unwrap();
    let transport =
        HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(30)).unwrap();
    let controller = Arc::new(DeviceController::new(transport, ControlLimits::unlimited()));
    controller.poll().await.unwrap();
    let sink = BatterySink::new(vec![Arc::clone(&controller)], Duration::MAX);
    fake.with_state(|state| {
        state.behaviour = FakeBehaviour::default()
            .without_spacing_rule()
            .with_report_delay(Duration::from_secs(60));
    })
    .unwrap();

    let started = tokio::time::Instant::now();
    let error = sink
        .hold(SN)
        .await
        .expect_err("the echo read must be bounded");
    assert!(
        matches!(&error, SinkError::ReadUnverified { .. }),
        "wrong timeout state: {error:?}"
    );
    assert!(
        started.elapsed() <= Duration::from_secs(9),
        "direct callers must not sit inside the 20 s pack budget: {:?}",
        started.elapsed()
    );
    fake.shutdown().await;
}
