//! Report ordering: only an ordered counter step can confirm or fault a command.

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
    Command, ControlFault, ControlLimits, DeviceController, ECHO_POLL_INTERVAL, FakeBehaviour,
    FakeDevice, FakeState, HttpTransport, Report,
};
use serde_json::json;
use support::{SN, ScriptedDevice, report_at};

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

fn echo(id: u32, timestamp: i64, input_w: u32) -> Report {
    report_at(
        SN,
        id,
        timestamp,
        json!({"acMode": 1, "inputLimit": input_w, "outputLimit": 0}),
    )
}

#[tokio::test]
async fn a_read_slower_than_three_beats_still_confirms() {
    let fake = FakeDevice::spawn(
        FakeState::new(SN).with_behaviour(FakeBehaviour::default().without_spacing_rule()),
    )
    .await
    .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(4)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    controller.poll().await.unwrap();
    // One read takes 3x a heartbeat: the beat must mark progress without cancelling that read.
    fake.with_state(|state| state.behaviour.report_delay = ECHO_POLL_INTERVAL * 3)
        .unwrap();
    controller
        .apply(Command::Charge { watts: 400 })
        .await
        .unwrap();
    let marks = Arc::new(AtomicUsize::new(0));
    let observation = {
        let marks = Arc::clone(&marks);
        controller
            .poll_confirming(Duration::from_secs(4), move || {
                marks.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap()
    };
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 400 }));
    assert!(
        marks.load(Ordering::Relaxed) >= 3,
        "the heartbeat marked {} times",
        marks.load(Ordering::Relaxed)
    );
    assert_eq!(fake.state().unwrap().write_count(), 1);
    fake.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_same_second_backward_reorder_is_inconclusive() {
    let (device, controller) = scripted(vec![baseline()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    device.push_report(echo(4213, T0 + 1, 0));
    device.push_report(echo(4212, T0 + 1, 300));
    let stale = controller.poll().await.unwrap();
    let reordered = controller.poll().await.unwrap();
    assert_eq!(stale.confirmed, None);
    assert_eq!(reordered.confirmed, None);
    assert!(reordered.fault.is_none());
    assert!(controller.pending().is_some());

    device.push_report(echo(4214, T0 + 2, 300));
    device.push_report(echo(4215, T0 + 3, 300));
    assert_eq!(
        controller
            .poll_confirming(Duration::from_secs(2), || {})
            .await
            .unwrap()
            .confirmed,
        Some(Command::Charge { watts: 300 })
    );
}

#[tokio::test(start_paused = true)]
async fn a_replay_after_several_wraps_is_inconclusive() {
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
    device.push_report(echo(253, T0 + 1, 0));
    device.push_report(echo(0, T0 + 2, 0));
    device.push_report(echo(253, T0 + 2, 500)); // replay from before the wrap
    for _ in 0..3 {
        let observation = controller.poll().await.unwrap();
        assert_eq!(observation.confirmed, None);
        assert!(
            observation.fault.is_none(),
            "the replay cannot fault the write"
        );
        assert!(controller.pending().is_some());
    }
    device.push_report(echo(3, T0 + 3, 500));
    device.push_report(echo(4, T0 + 4, 500));
    assert_eq!(
        controller
            .poll_confirming(Duration::from_secs(2), || {})
            .await
            .unwrap()
            .confirmed,
        Some(Command::Charge { watts: 500 })
    );
}

#[tokio::test(start_paused = true)]
async fn a_thirty_three_step_burst_resynchronises_without_confirming() {
    let (device, controller) = scripted(vec![report_at(
        SN,
        100,
        T0,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    )]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    device.push_report(echo(133, T0 + 1, 300)); // 33 steps: resync, not evidence
    let burst = controller.poll().await.unwrap();
    assert_eq!(burst.confirmed, None);
    assert!(burst.fault.is_none() && controller.pending().is_some());

    device.push_report(echo(134, T0 + 2, 300));
    device.push_report(echo(135, T0 + 3, 300));
    assert_eq!(
        controller
            .poll_confirming(Duration::from_secs(2), || {})
            .await
            .unwrap()
            .confirmed,
        Some(Command::Charge { watts: 300 })
    );
}

#[tokio::test]
async fn a_delayed_read_spans_a_burst_and_still_recovers() {
    let fake = FakeDevice::spawn(
        FakeState::new(SN).with_behaviour(FakeBehaviour::default().without_spacing_rule()),
    )
    .await
    .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(8)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    fake.with_state(|state| {
        state.message_id = 4211 + 33;
        state.timestamp += 33;
        state.behaviour.report_delay = Duration::from_millis(1_500);
    })
    .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(10), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 300 }));
    assert!(controller.pending().is_none());
    fake.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_thirty_two_step_wrap_confirms_but_thirty_three_resynchronises() {
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
    device.push_report(echo(26, T0 + 1, 500)); // 250 -> 26 is exactly 32 steps
    device.push_report(echo(27, T0 + 2, 500));
    assert_eq!(
        controller
            .poll_confirming(Duration::from_secs(2), || {})
            .await
            .unwrap()
            .confirmed,
        Some(Command::Charge { watts: 500 })
    );

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
    device.push_report(echo(27, T0 + 1, 500)); // 33 steps: resync only
    let burst = controller.poll().await.unwrap();
    assert_eq!(burst.confirmed, None);
    assert!(burst.fault.is_none() && controller.pending().is_some());
    device.push_report(echo(28, T0 + 2, 500));
    device.push_report(echo(29, T0 + 3, 500));
    assert_eq!(
        controller
            .poll_confirming(Duration::from_secs(2), || {})
            .await
            .unwrap()
            .confirmed,
        Some(Command::Charge { watts: 500 })
    );
}

#[tokio::test(start_paused = true)]
async fn a_reset_with_an_equal_or_older_clock_is_inconclusive() {
    let (device, controller) = scripted(vec![baseline()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    device.push_report(echo(0, T0, 300));
    device.push_report(echo(1, T0 - 1, 300));
    for _ in 0..2 {
        let observation = controller.poll().await.unwrap();
        assert_eq!(observation.confirmed, None);
        assert!(
            observation.fault.is_none(),
            "a reset without a newer clock is not proof of a reboot"
        );
        assert!(controller.pending().is_some());
    }
    let expired = controller
        .poll_confirming(Duration::from_millis(100), || {})
        .await
        .unwrap();
    assert!(matches!(
        expired.fault,
        Some(ControlFault::ConfirmationTimeout { .. })
    ));
    assert!(controller.pending().is_none());
}
