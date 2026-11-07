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
use std::time::Duration;

use energy_battery::{
    BatteryError, Command, ControlFault, ControlLimits, DeviceController, Report,
};
use serde_json::{Value, json};
use support::gated::GatedDevice;
use support::{SN, ScriptedDevice, report_at};

/// The clock of the first scripted report; each later one is a second ahead.
const T0: i64 = 1_791_959_470;

/// The `n`th scripted report: counter and device clock both move forward, as real reports do.
fn echo(n: u32, overrides: Value) -> Report {
    report_at(SN, 4211 + n, T0 + i64::from(n), overrides)
}

fn fast_controller(
    reports: Vec<Report>,
) -> (Arc<ScriptedDevice>, DeviceController<Arc<ScriptedDevice>>) {
    let device = Arc::new(ScriptedDevice::new(SN, reports));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
        .with_spacing(Duration::ZERO);
    (device, controller)
}

#[tokio::test]
async fn a_duplicate_report_cannot_confirm_a_command() {
    // H10: the zero limits of the report that was already there before the hold used to "confirm"
    // it, so a hold that never took looked applied.
    let (device, controller) = fast_controller(vec![echo(
        0,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    )]);
    controller.poll().await.unwrap();
    controller.apply(Command::Hold).await.unwrap();

    device.push_report(echo(
        0,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    ));
    let observation = controller.poll().await.unwrap();
    assert_eq!(
        observation.confirmed, None,
        "a repeated report is not an echo"
    );
    assert!(observation.is_healthy(), "a duplicate is not a fault");
    assert!(
        controller.pending().is_some(),
        "a duplicate must not clear ownership of the command"
    );

    // A genuinely newer report that still shows the old limit is the real fault.
    device.push_report(echo(
        1,
        json!({"acMode": 2, "inputLimit": 500, "outputLimit": 0}),
    ));
    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::HoldEcho { input_w: 500, .. })
    ));
}

#[tokio::test]
async fn a_report_for_another_device_cannot_confirm_a_command() {
    let (device, controller) = fast_controller(vec![echo(0, json!({"acMode": 2}))]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();

    device.push_report(report_at(
        "EOC1NLN9N4YYYYYY",
        4212,
        T0 + 1,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::ForeignReport { .. })
    ));
    assert_eq!(observation.confirmed, None);
    assert!(
        controller.pending().is_some(),
        "a foreign report must not clear ownership"
    );
}

#[tokio::test]
async fn a_command_is_pending_before_its_write_is_transmitted() {
    // H9: the reservation used to be installed after the write returned, so a report that arrived
    // while the write was in flight had nothing to verify.
    let device = Arc::new(GatedDevice::new(SN));
    device.push_report(echo(0, json!({"acMode": 2})));
    let controller = Arc::new(DeviceController::new(
        Arc::clone(&device),
        ControlLimits::unlimited(),
    ));
    controller.poll().await.unwrap();

    let writing = tokio::spawn({
        let controller = Arc::clone(&controller);
        async move { controller.apply(Command::Charge { watts: 500 }).await }
    });
    device.write_started().await;
    assert_eq!(
        controller.pending().map(|pending| pending.command),
        Some(Command::Charge { watts: 500 }),
        "the command must be reserved before the request leaves"
    );
    device.release();
    writing.await.unwrap().unwrap();
    assert_eq!(device.writes(), 1);

    device.push_report(echo(
        1,
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0}),
    ));
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 500 }));
}

#[tokio::test]
async fn an_unresolved_command_cannot_be_replaced() {
    let (device, controller) = fast_controller(vec![echo(0, json!({"acMode": 2}))]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();

    let refused = controller.apply(Command::Charge { watts: 400 }).await;
    assert!(
        matches!(refused, Err(BatteryError::Unconfirmed { .. })),
        "a second writer must not overwrite an unverified command, got {refused:?}"
    );
    assert_eq!(device.write_count(), 1);

    device.push_report(echo(
        1,
        json!({"acMode": 1, "inputLimit": 300, "outputLimit": 0}),
    ));
    assert_eq!(
        controller.poll().await.unwrap().confirmed,
        Some(Command::Charge { watts: 300 })
    );
    controller
        .apply(Command::Charge { watts: 400 })
        .await
        .unwrap();
}
