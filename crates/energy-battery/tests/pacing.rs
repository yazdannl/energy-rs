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

use energy_battery::{BatteryError, Command, ControlLimits, DeviceController, MIN_WRITE_SPACING};
use serde_json::json;
use support::{SN, ScriptedDevice, report_at, report_with};

fn idle_report() -> energy_battery::Report {
    report_with(SN, json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}))
}

/// A newer report that echoes a charge command, so the command before it is resolved.
fn charge_echo(input_limit: u32) -> energy_battery::Report {
    report_at(
        SN,
        4212,
        1_791_959_471,
        json!({"acMode": 1, "inputLimit": input_limit, "outputLimit": 0}),
    )
}

/// A newer report that echoes a discharge command.
fn discharge_echo(output_limit: u32) -> energy_battery::Report {
    report_at(
        SN,
        4212,
        1_791_959_471,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": output_limit}),
    )
}

#[tokio::test(start_paused = true)]
async fn the_second_write_waits_for_the_pacing_floor() {
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![idle_report(), charge_echo(300)],
    ));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    controller.poll().await.unwrap();

    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    // A command in doubt blocks the next one until a report resolves it.
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 400 })
        .await
        .unwrap();

    let times = device.write_times();
    assert_eq!(times.len(), 2);
    let gap = times[1].saturating_duration_since(times[0]);
    assert_eq!(gap, MIN_WRITE_SPACING, "pacer slept {gap:?}");
}

#[tokio::test(start_paused = true)]
async fn concurrent_writes_never_replace_an_unresolved_command() {
    let device = Arc::new(ScriptedDevice::new(SN, vec![idle_report()]));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    controller.poll().await.unwrap();

    let first = controller.apply(Command::Charge { watts: 300 });
    let second = controller.apply(Command::Charge { watts: 400 });
    let (first, second) = tokio::join!(first, second);
    first.unwrap();
    assert!(
        matches!(second, Err(BatteryError::Unconfirmed { .. })),
        "the queued writer must not overwrite the in-doubt command, got {second:?}"
    );
    assert_eq!(device.write_count(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_custom_spacing_is_honoured() {
    let device = Arc::new(ScriptedDevice::new(SN, vec![idle_report()]));
    let spacing = Duration::from_millis(500);
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
        .with_spacing(spacing);
    assert_eq!(controller.spacing(), spacing);
    controller.poll().await.unwrap();

    controller
        .apply(Command::Discharge { watts: 300 })
        .await
        .unwrap();
    device.push_report(discharge_echo(300));
    assert_eq!(
        controller.poll().await.unwrap().confirmed,
        Some(Command::Discharge { watts: 300 })
    );
    controller
        .apply(Command::Discharge { watts: 350 })
        .await
        .unwrap();

    let times = device.write_times();
    assert_eq!(times[1].saturating_duration_since(times[0]), spacing);
}

#[tokio::test(start_paused = true)]
async fn writes_are_refused_until_the_device_was_read() {
    let device = Arc::new(ScriptedDevice::new(SN, vec![idle_report()]));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    let error = controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("never read"), "{error}");
    assert_eq!(device.write_count(), 0);
}
