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
    BatteryError, Command, ControlFault, ControlLimits, DeviceController, ECHO_CONFIRM_WINDOW,
    FakeBehaviour, FakeDevice, FakeState, HttpTransport, Report, SAFETY_HOLD_WINDOW,
};
use serde_json::json;
use support::broken_reads::BrokenReads;
use support::{SN, ScriptedDevice, report_at};
use tokio::time::Instant;

const T0: i64 = 1_791_959_470;

fn scripted(reports: Vec<Report>) -> (Arc<ScriptedDevice>, DeviceController<Arc<ScriptedDevice>>) {
    let device = Arc::new(ScriptedDevice::new(SN, reports));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
        .with_spacing(Duration::ZERO);
    (device, controller)
}

fn baseline() -> Report {
    report_at(
        SN,
        4211,
        T0,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    )
}

async fn fake_with(behaviour: FakeBehaviour) -> (FakeDevice, DeviceController<HttpTransport>) {
    let fake = FakeDevice::spawn(FakeState::new(SN).with_behaviour(behaviour))
        .await
        .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(3)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    (fake, controller)
}

#[tokio::test(start_paused = true)]
async fn delayed_application_confirms_after_four_old_limit_reports() {
    let (fake, controller) = fake_with(
        FakeBehaviour::default()
            .without_spacing_rule()
            .with_stale_echo_reports(4),
    )
    .await;
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
    assert!(marks.load(Ordering::Relaxed) >= 4);
    assert_eq!(fake.state().unwrap().write_count(), 1);
    fake.shutdown().await;
}

#[tokio::test]
async fn a_slow_read_marks_progress_and_a_late_echo_is_rejected() {
    let (fake, controller) = fake_with(FakeBehaviour::default().without_spacing_rule()).await;
    controller.poll().await.unwrap();
    fake.with_state(|state| state.behaviour.report_delay = Duration::from_millis(1_200))
        .unwrap();
    controller
        .apply(Command::Discharge { watts: 700 })
        .await
        .unwrap();

    let marks = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let observation = {
        let marks = Arc::clone(&marks);
        controller
            .poll_confirming(Duration::from_millis(800), move || {
                marks.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap()
    };
    assert!(
        Instant::now().duration_since(started) <= Duration::from_millis(950),
        "the deadline must be hard"
    );
    assert!(
        marks.load(Ordering::Relaxed) >= 2,
        "heartbeat during the read"
    );
    assert!(matches!(
        observation.fault,
        Some(ControlFault::ConfirmationTimeout { .. })
    ));
    assert!(
        controller.pending().is_none(),
        "a late echo cannot revive it"
    );
    fake.shutdown().await;
}

#[tokio::test]
async fn a_stalled_transport_releases_the_command_without_a_fault() {
    let device = Arc::new(BrokenReads::new(SN));
    device.start_healthy();
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
        .with_spacing(Duration::ZERO);
    controller.poll().await.unwrap();
    device.break_reads();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();

    let observation = controller
        .poll_confirming(Duration::from_millis(100), || {})
        .await
        .unwrap();
    assert_eq!(observation.fault, None, "a read miss is not a write fault");
    assert!(
        observation
            .read_error
            .as_deref()
            .is_some_and(|detail| detail.contains("stalled transport")),
        "{:?}",
        observation.read_error
    );
    assert!(controller.pending().is_none());
}

#[tokio::test(start_paused = true)]
async fn several_counter_wraps_and_an_ack_wrap_still_confirm() {
    let (device, controller) = scripted(vec![report_at(
        SN,
        250,
        T0,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    )]);
    device.set_ack_id(1); // 250 -> 1 across the 8-bit wrap.
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    for (id, timestamp) in [(253, T0 + 1), (0, T0 + 2), (3, T0 + 3), (6, T0 + 4)] {
        device.push_report(report_at(
            SN,
            id,
            timestamp,
            json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0}),
        ));
    }
    device.push_report(report_at(
        SN,
        9,
        T0 + 5,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 500 }));
    assert!(observation.fault.is_none());
}

#[tokio::test(start_paused = true)]
async fn a_replayed_report_after_a_wrap_never_confirms() {
    let (device, controller) = scripted(vec![report_at(
        SN,
        250,
        T0,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    )]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    // First report after the wrap is stale; the replay carries the echo but the same counter.
    device.push_report(report_at(
        SN,
        0,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0}),
    ));
    device.push_report(report_at(
        SN,
        0,
        T0 + 2,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    let stale = controller.poll().await.unwrap();
    let replay = controller.poll().await.unwrap();
    assert_eq!(replay.confirmed, None, "a replayed counter cannot confirm");
    assert!(replay.fault.is_none() && stale.fault.is_none());
    assert!(controller.pending().is_some());

    let expired = controller
        .poll_confirming(Duration::from_millis(100), || {})
        .await
        .unwrap();
    assert_eq!(expired.confirmed, None);
    assert!(controller.pending().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_safety_hold_preempts_an_unresolved_write_after_the_bound() {
    let (device, controller) = scripted(vec![baseline()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    assert!(matches!(
        controller.apply(Command::Charge { watts: 400 }).await,
        Err(BatteryError::Unconfirmed { .. })
    ));

    let started = Instant::now();
    controller.apply(Command::Hold).await.unwrap();
    assert!(
        Instant::now().duration_since(started) <= SAFETY_HOLD_WINDOW + Duration::from_millis(50),
        "a hold must not be blocked indefinitely"
    );
    assert_eq!(device.write_count(), 2);
    assert_eq!(controller.pending().unwrap().command, Command::Hold);
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    ));
    assert_eq!(
        controller.poll().await.unwrap().confirmed,
        Some(Command::Hold)
    );
}

#[tokio::test(start_paused = true)]
async fn a_waiter_never_expires_a_command_that_replaced_it() {
    let (device, controller) = scripted(vec![baseline()]);
    let controller = Arc::new(controller);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    let waiter = tokio::spawn({
        let controller = Arc::clone(&controller);
        async move {
            controller
                .poll_confirming(Duration::from_secs(5), || {})
                .await
        }
    });
    controller.apply(Command::Hold).await.unwrap();
    let replacement = controller.pending().unwrap();
    assert_eq!(replacement.command, Command::Hold);

    let outcome = waiter.await.unwrap();
    assert!(
        outcome.is_ok_and(|observation| observation.fault.is_none()),
        "the old waiter must not fault the replacement"
    );
    let still = controller.pending().expect("the hold stays pending");
    assert_eq!(still.request_id, replacement.request_id);

    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    ));
    assert_eq!(
        controller.poll().await.unwrap().confirmed,
        Some(Command::Hold)
    );
}

#[tokio::test(start_paused = true)]
async fn mismatch_then_duplicate_only_confirms_on_the_next_fresh_report() {
    let (device, controller) = scripted(vec![baseline()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0}),
    ));
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 300, "outputLimit": 0}),
    ));
    device.push_report(report_at(
        SN,
        4213,
        T0 + 2,
        json!({"acMode": 1, "inputLimit": 300, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 300 }));
    assert_eq!(
        observation.report.message_id, 4213,
        "the fresh echo confirmed"
    );
}

#[tokio::test(start_paused = true)]
async fn mismatch_then_a_foreign_report_keeps_ownership_of_the_command() {
    let (device, controller) = scripted(vec![baseline()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0}),
    ));
    device.push_report(report_at(
        "EOC1NLN9N4YYYYYY",
        4213,
        T0 + 2,
        json!({"acMode": 1, "inputLimit": 300, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_secs(1), || {})
        .await
        .unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::ForeignReport { .. })
    ));
    assert_eq!(observation.confirmed, None);
    controller.apply(Command::Hold).await.unwrap();
    assert_eq!(controller.pending().unwrap().command, Command::Hold);
}

#[tokio::test(start_paused = true)]
async fn mismatch_then_a_safety_fault_reports_once_and_releases_the_command() {
    let (device, controller) = scripted(vec![baseline()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    device.push_report(report_at(
        SN,
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0}),
    ));
    device.push_report(report_at(
        SN,
        4213,
        T0 + 2,
        json!({"acMode": 127, "inputLimit": 0, "outputLimit": 0}),
    ));
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::Wedged { ac_mode: 127, .. })
    ));
    assert!(controller.pending().is_none());
}
