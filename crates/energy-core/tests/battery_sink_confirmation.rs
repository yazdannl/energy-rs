//! Caller-level coverage: the dispatcher's real battery sink, pacing and fail-safe hold.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use energy_battery::{
    ControlLimits, DeviceController, FakeBehaviour, FakeDevice, FakeState, HttpTransport,
};
use energy_core::dispatch::{
    BatterySink, CommandSink, Dispatcher, FleetView, MeterReading, Overrides, Progress, TickInput,
};
use energy_core::safety::{Authority, SafetyLimits};
use support::fakes::{NOW, pack, plan};

const SN: &str = "SN-A";

struct Marks(AtomicUsize);

impl Progress for Marks {
    fn mark(&self, _now: i64) {
        self.0.fetch_add(1, Ordering::Relaxed);
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
    let sink =
        Arc::new(BatterySink::new(vec![Arc::clone(&controller)]).with_confirm_window(window));
    (fake, sink, controller)
}

#[tokio::test]
async fn one_unconfirmed_dispatch_gets_one_hold_and_marks_progress() {
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
    assert!(report.fail_safe);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("not confirmed")),
        "{:?}",
        report.error
    );
    let state = fake.state().unwrap();
    assert_eq!(state.write_count(), 2, "one setpoint write and one hold");
    assert_eq!(state.writes[0]["properties"]["outputLimit"], 800);
    assert_eq!(state.writes[1]["properties"]["outputLimit"], 0);
    assert!(
        marks.0.load(Ordering::Relaxed) > 0,
        "the wait must mark progress"
    );
    fake.shutdown().await;
}

#[tokio::test]
async fn a_hold_requested_during_confirmation_is_not_blocked() {
    let (fake, sink, controller) = fixture(
        FakeBehaviour::default()
            .without_spacing_rule()
            .ignoring_writes(),
        Duration::from_secs(2),
    )
    .await;
    let writing = {
        let sink = Arc::clone(&sink);
        tokio::spawn(async move { sink.write(SN, -800.0).await })
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        while controller.pending().is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the write became pending");
    let held = sink.hold(SN).await.expect("hold succeeds");
    assert!(held.verified);
    assert!(
        writing.await.unwrap().is_err(),
        "the superseded write fails"
    );
    assert_eq!(
        fake.state().unwrap().write_count(),
        2,
        "the hold was sent once, not stormed"
    );
    fake.shutdown().await;
}
