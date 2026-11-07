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

use energy_battery::{
    BatteryError, Command, ControlFault, ControlLimits, DeviceController, Direction,
    ECHO_TOLERANCE_W, FAULT_BACKOFF,
};
use serde_json::json;
use support::{SN, ScriptedDevice, report_with};
use tokio::time::Instant;

fn controller_with(
    reports: Vec<energy_battery::Report>,
) -> (Arc<ScriptedDevice>, DeviceController<Arc<ScriptedDevice>>) {
    let device = Arc::new(ScriptedDevice::new(SN, reports));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    (device, controller)
}

#[tokio::test]
async fn a_write_is_confirmed_by_the_next_report() {
    let (_, controller) = controller_with(vec![
        report_with(SN, json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0})),
        report_with(
            SN,
            json!({"acMode": 1, "inputLimit": 650, "outputLimit": 0}),
        ),
    ]);
    controller.poll().await.unwrap();
    let applied = controller
        .apply(Command::Charge { watts: 650 })
        .await
        .unwrap();
    assert_eq!(applied.command, Command::Charge { watts: 650 });
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 650 }));
    assert!(observation.is_healthy());
    assert!(controller.pending().is_none());
}

#[tokio::test]
async fn an_echo_outside_the_tolerance_is_a_typed_fault() {
    let (_, controller) = controller_with(vec![
        report_with(SN, json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0})),
        report_with(
            SN,
            json!({"acMode": 1, "inputLimit": 400, "outputLimit": 0}),
        ),
    ]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 650 })
        .await
        .unwrap();
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, None);
    match observation.fault {
        Some(ControlFault::EchoMismatch {
            direction,
            expected_w,
            echoed_w,
            ..
        }) => {
            assert_eq!(direction, Direction::Charge);
            assert_eq!(expected_w, 650);
            assert_eq!(echoed_w, 400);
        }
        other => panic!("expected EchoMismatch, got {other:?}"),
    }
    assert!(controller.last_fault().is_some());
}

#[tokio::test]
async fn an_echo_inside_the_tolerance_is_confirmed() {
    let echo = 650 - ECHO_TOLERANCE_W;
    let (_, controller) = controller_with(vec![
        report_with(SN, json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0})),
        report_with(
            SN,
            json!({"acMode": 1, "inputLimit": echo, "outputLimit": 0}),
        ),
    ]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 650 })
        .await
        .unwrap();
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 650 }));
}

#[tokio::test]
async fn a_hold_is_confirmed_by_zero_limits() {
    let (device, controller) = controller_with(vec![
        report_with(
            SN,
            json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
        ),
        report_with(SN, json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0})),
    ]);
    controller.poll().await.unwrap();
    controller.apply(Command::Hold).await.unwrap();
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, Some(Command::Hold));

    let writes = device.writes();
    let properties = writes[0].properties();
    assert_eq!(properties.ac_mode(), None, "a hold must not write acMode");
    assert_eq!(properties.input_limit(), Some(0));
    assert_eq!(properties.output_limit(), Some(0));
    assert_eq!(properties.smart_mode(), Some(1));
}

#[tokio::test]
async fn a_hold_that_did_not_take_is_a_typed_fault() {
    let (_, controller) = controller_with(vec![
        report_with(
            SN,
            json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
        ),
        report_with(
            SN,
            json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
        ),
    ]);
    controller.poll().await.unwrap();
    controller.apply(Command::Hold).await.unwrap();
    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::HoldEcho {
            input_w: 500,
            output_w: 0,
            ..
        })
    ));
}

#[tokio::test]
async fn the_wrong_direction_after_a_write_is_a_fault() {
    let (_, controller) = controller_with(vec![
        report_with(SN, json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0})),
        report_with(
            SN,
            json!({"acMode": 2, "inputLimit": 650, "outputLimit": 0}),
        ),
    ]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 650 })
        .await
        .unwrap();
    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::WrongMode {
            expected: Direction::Charge,
            ac_mode: 2,
            ..
        })
    ));
}

#[tokio::test]
async fn a_wedged_device_faults_and_refuses_writes() {
    let (device, controller) = controller_with(vec![report_with(SN, json!({"acMode": 127}))]);
    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::Wedged { ac_mode: 127, .. })
    ));
    assert!(controller.is_wedged());

    let error = controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        BatteryError::Fault(ControlFault::Wedged { ac_mode: 127, .. })
    ));
    assert_eq!(device.write_count(), 0);
}

#[tokio::test]
async fn a_device_that_recovers_clears_the_wedge() {
    let (_, controller) = controller_with(vec![
        report_with(SN, json!({"acMode": 255})),
        report_with(SN, json!({"acMode": 2})),
    ]);
    controller.poll().await.unwrap();
    assert!(controller.is_wedged());
    let observation = controller.poll().await.unwrap();
    assert!(observation.is_healthy());
    assert!(!controller.is_wedged());
}

#[tokio::test]
async fn a_report_counter_that_went_backwards_is_a_restart() {
    let (device, controller) = controller_with(vec![report_with(SN, json!({"acMode": 2}))]);
    let first = controller.poll().await.unwrap();
    assert_eq!(first.report.message_id, 4211);

    let mut second = report_with(SN, json!({"acMode": 2}));
    second.message_id = 3;
    device.push_report(second);
    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::Restarted {
            before: 4211,
            after: 3,
            ..
        })
    ));
}

#[tokio::test(start_paused = true)]
async fn a_restart_backs_off_before_the_next_write() {
    let (device, controller) = controller_with(vec![report_with(SN, json!({"acMode": 2}))]);
    controller.poll().await.unwrap();

    let mut restarted = report_with(SN, json!({"acMode": 2}));
    restarted.message_id = 3;
    device.push_report(restarted);
    let restart_at = Instant::now();
    let observation = controller.poll().await.unwrap();
    assert!(observation.fault.is_some());

    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    let write_at = device.write_times()[0];
    let waited = write_at.saturating_duration_since(restart_at);
    assert!(
        waited >= FAULT_BACKOFF,
        "wrote {waited:?} after the restart"
    );
}

#[tokio::test]
async fn queued_reports_are_consumed_in_order_and_the_last_one_repeats() {
    let (_, controller) = controller_with(vec![
        report_with(SN, json!({"acMode": 2})),
        report_with(SN, json!({"acMode": 1})),
    ]);
    assert_eq!(
        controller.poll().await.unwrap().report.properties.ac_mode,
        2
    );
    assert_eq!(
        controller.poll().await.unwrap().report.properties.ac_mode,
        1
    );
    assert_eq!(
        controller.poll().await.unwrap().report.properties.ac_mode,
        2
    );
}
