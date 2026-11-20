// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
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
    Command, ControlFault, ControlLimits, DeviceController, ECHO_CONFIRM_WINDOW, FakeBehaviour,
    FakeDevice, FakeState, HttpTransport,
};
use serde_json::json;
use support::read_flake::ReadFlake;
use support::{SN, ScriptedDevice, report_at};
use tokio::time::Instant;

/// Clock of the scripted baseline reports.
const T0: i64 = 1_791_959_470;

fn baseline(device: Arc<ScriptedDevice>) -> DeviceController<Arc<ScriptedDevice>> {
    DeviceController::new(device, ControlLimits::unlimited()).with_spacing(Duration::ZERO)
}

async fn ignoring_device() -> (FakeDevice, DeviceController<HttpTransport>) {
    let fake = FakeDevice::spawn(
        FakeState::new(SN).with_behaviour(
            FakeBehaviour::default()
                .without_spacing_rule()
                .ignoring_writes(),
        ),
    )
    .await
    .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(2)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    (fake, controller)
}

async fn charge_pending() -> (Arc<ScriptedDevice>, DeviceController<Arc<ScriptedDevice>>) {
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(
            SN,
            4211,
            T0,
            json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
        )],
    ));
    let controller = baseline(Arc::clone(&device));
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    (device, controller)
}

#[tokio::test(start_paused = true)]
async fn a_stale_report_after_the_ack_waits_for_the_real_limit() {
    // The real 2026-11-18 race: the write is acknowledged, the next report has a newer messageId
    // but still shows outputLimit 0, and the applied limit appears one report later.
    let fake = FakeDevice::spawn(
        FakeState::new(SN).with_behaviour(
            FakeBehaviour::default()
                .without_spacing_rule()
                .with_stale_echo_reports(1),
        ),
    )
    .await
    .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(2)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 700 })
        .await
        .unwrap();

    let marks = Arc::new(AtomicUsize::new(0));
    let observation = {
        let marks = Arc::clone(&marks);
        controller
            .poll_confirming(ECHO_CONFIRM_WINDOW, move || {
                marks.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap()
    };
    assert_eq!(
        observation.confirmed,
        Some(Command::Discharge { watts: 700 })
    );
    assert!(observation.fault.is_none(), "{:?}", observation.fault);
    assert!(
        marks.load(Ordering::Relaxed) >= 1,
        "the wait must mark the engine heartbeat"
    );
    let state = fake.state().unwrap();
    assert_eq!(state.output_limit_w, 700);
    assert_eq!(state.write_count(), 1, "one write, confirmed once");
    fake.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_never_applied_write_faults_once_within_the_bound() {
    let (fake, controller) = ignoring_device().await;
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 700 })
        .await
        .unwrap();

    let marks = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let observation = {
        let marks = Arc::clone(&marks);
        controller
            .poll_confirming(ECHO_CONFIRM_WINDOW, move || {
                marks.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap()
    };
    let waited = Instant::now().duration_since(started);
    assert!(
        waited <= ECHO_CONFIRM_WINDOW + Duration::from_millis(100),
        "{waited:?}"
    );
    assert_eq!(observation.confirmed, None);
    assert!(
        matches!(
            observation.fault,
            Some(ControlFault::EchoMismatch {
                expected_w: 700,
                echoed_w: 0,
                ..
            })
        ),
        "{:?}",
        observation.fault
    );
    assert!(marks.load(Ordering::Relaxed) >= 1);
    assert!(
        controller.pending().is_none(),
        "the command must be released"
    );
    // The expired doubt does not produce a second fault on the next poll.
    assert!(controller.poll().await.unwrap().fault.is_none());
    fake.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_expired_write_gets_exactly_one_fail_safe_hold() {
    let (fake, controller) = ignoring_device().await;
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 700 })
        .await
        .unwrap();
    let failed = controller
        .poll_confirming(ECHO_CONFIRM_WINDOW, || {})
        .await
        .unwrap();
    assert!(failed.fault.is_some());

    // The dispatcher's one fail-safe hold, sent and confirmed through the same path.
    controller.apply(Command::Hold).await.unwrap();
    let held = controller
        .poll_confirming(ECHO_CONFIRM_WINDOW, || {})
        .await
        .unwrap();
    assert_eq!(held.confirmed, Some(Command::Hold));
    assert_eq!(
        fake.state().unwrap().write_count(),
        2,
        "one setpoint write and one fail-safe hold"
    );
    fake.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_duplicate_report_never_confirms_during_the_wait() {
    let (device, controller) = charge_pending().await;
    // A duplicate of the pre-write report, even with the requested limit, proves nothing.
    device.push_report(report_at(
        SN,
        4211,
        T0,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_millis(100), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, None);
    assert!(observation.fault.is_none());
    assert_eq!(
        controller.pending().map(|pending| pending.command),
        Some(Command::Charge { watts: 500 })
    );

    // Ownership is still with the command, so its own fresh echo can confirm it.
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    assert_eq!(
        controller.poll().await.unwrap().confirmed,
        Some(Command::Charge { watts: 500 })
    );
}

#[tokio::test(start_paused = true)]
async fn a_foreign_report_never_confirms_during_the_wait() {
    let (device, controller) = charge_pending().await;
    // A foreign report with a perfectly matching echo is not evidence about this pack.
    device.push_report(report_at(
        "EOC1NLN9N4YYYYYY",
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_millis(100), || {})
        .await
        .unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::ForeignReport { .. })
    ));
    assert_eq!(observation.confirmed, None);
    assert!(controller.pending().is_some());

    device.push_report(report_at(
        SN,
        4213,
        T0 + 2,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    assert_eq!(
        controller.poll().await.unwrap().confirmed,
        Some(Command::Charge { watts: 500 })
    );
}

#[tokio::test(start_paused = true)]
async fn zero_of_the_other_direction_is_not_a_hold_echo() {
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(
            SN,
            4211,
            T0,
            json!({"acMode": 2, "inputLimit": 0, "outputLimit": 500}),
        )],
    ));
    let controller = baseline(Arc::clone(&device));
    controller.poll().await.unwrap();
    controller.apply(Command::Hold).await.unwrap();

    // Only inputLimit is zero: that is the running discharge's opposite direction, not a hold.
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 500}),
    ));
    device.push_report(report_at(
        SN,
        4213,
        T0 + 2,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(
        observation.confirmed,
        Some(Command::Hold),
        "the half-zero report is a doubt, the all-zero report is the echo"
    );
    assert!(observation.fault.is_none());
}

#[tokio::test(start_paused = true)]
async fn a_discharge_is_not_confused_with_a_hold_by_zero_input_limit() {
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(
            SN,
            4211,
            T0,
            json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
        )],
    ));
    let controller = baseline(Arc::clone(&device));
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 600 })
        .await
        .unwrap();

    // Both limits zero looks exactly like a hold, but it is not this command's echo.
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    ));
    device.push_report(report_at(
        SN,
        4213,
        T0 + 2,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 600}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(
        observation.confirmed,
        Some(Command::Discharge { watts: 600 })
    );
    assert!(observation.fault.is_none());
}

#[tokio::test(start_paused = true)]
async fn a_transient_read_miss_is_retried_until_the_echo_arrives() {
    let scripted = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(
            SN,
            4211,
            T0,
            json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
        )],
    ));
    let device = Arc::new(ReadFlake::new(Arc::clone(&scripted), 0));
    let controller = DeviceController::new(device.clone(), ControlLimits::unlimited())
        .with_spacing(Duration::ZERO);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 700 })
        .await
        .unwrap();

    // The first confirmation read times out; a later report carries the real echo.
    device.set_misses(1);
    scripted.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 700}),
    ));
    let observation = controller
        .poll_confirming(ECHO_CONFIRM_WINDOW, || {})
        .await
        .unwrap();
    assert_eq!(
        observation.confirmed,
        Some(Command::Discharge { watts: 700 })
    );
    assert!(observation.fault.is_none());
    assert!(observation.read_error.is_none());
    assert!(controller.pending().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_window_of_read_misses_ends_without_a_device_fault() {
    let scripted = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(
            SN,
            4211,
            T0,
            json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
        )],
    ));
    let device = Arc::new(ReadFlake::new(Arc::clone(&scripted), 0));
    let controller = DeviceController::new(device.clone(), ControlLimits::unlimited())
        .with_spacing(Duration::ZERO);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 700 })
        .await
        .unwrap();
    device.set_misses(100);

    let marks = Arc::new(AtomicUsize::new(0));
    let observation = {
        let marks = Arc::clone(&marks);
        controller
            .poll_confirming(Duration::from_millis(200), move || {
                marks.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap()
    };
    assert_eq!(observation.confirmed, None);
    assert!(observation.fault.is_none(), "{:?}", observation.fault);
    assert!(
        observation
            .read_error
            .as_deref()
            .is_some_and(|error| error.contains("scripted read timeout")),
        "{observation:?}"
    );
    assert!(marks.load(Ordering::Relaxed) >= 1);
    assert!(
        controller.pending().is_none(),
        "the next tick must not be refused as Unconfirmed"
    );
}
