//! Flow streaks are consecutive: an echo gap or a mixed full/low run cannot confirm full flow.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal
)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use energy_battery::{Command, ControlLimits, DeviceController};
use serde_json::json;
use support::{SN, ScriptedDevice, report_at};

const T0: i64 = 1_791_959_470;

fn controller(
    reports: Vec<energy_battery::Report>,
) -> (Arc<ScriptedDevice>, DeviceController<Arc<ScriptedDevice>>) {
    let device = Arc::new(ScriptedDevice::new(SN, reports));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
        .with_spacing(Duration::ZERO);
    (device, controller)
}

fn idle() -> energy_battery::Report {
    report_at(
        SN,
        1,
        T0,
        json!({"acMode": 1, "inputLimit": 0, "outputLimit": 0, "acStatus": 0, "dcStatus": 0}),
    )
}

fn charged(id: u32, grid_w: i64) -> energy_battery::Report {
    report_at(
        SN,
        id,
        T0 + i64::from(id),
        json!({"acMode": 1, "inputLimit": 500, "outputLimit": 0, "gridInputPower": grid_w}),
    )
}

fn stale_echo(id: u32) -> energy_battery::Report {
    report_at(
        SN,
        id,
        T0 + i64::from(id),
        json!({
            "acMode": 1,
            "inputLimit": 0,
            "outputLimit": 0,
            "acStatus": 2,
            "dcStatus": 1,
            "gridInputPower": 500
        }),
    )
}

#[tokio::test]
async fn an_inconclusive_echo_restarts_the_full_flow_streak() {
    let (device, controller) = controller(vec![idle()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    device.push_report(charged(2, 500));
    device.push_report(stale_echo(3));
    device.push_report(charged(4, 500));
    device.push_report(charged(5, 500));

    let first = controller.poll().await.unwrap();
    assert_eq!(first.confirmed, None);
    let gap = controller.poll().await.unwrap();
    assert_eq!(gap.confirmed, None);
    assert!(gap.fault.is_none());
    let one_after_gap = controller.poll().await.unwrap();
    assert_eq!(
        one_after_gap.confirmed, None,
        "the streak must restart at the gap"
    );
    let recovered = controller.poll().await.unwrap();
    assert_eq!(recovered.confirmed, Some(Command::Charge { watts: 500 }));
}

#[tokio::test]
async fn a_curtailed_then_full_run_resolves_partially_not_as_full_flow() {
    let (device, controller) = controller(vec![idle()]);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    device.push_report(charged(2, 45));
    device.push_report(charged(3, 500));

    let low = controller.poll().await.unwrap();
    assert_eq!(low.confirmed, None);
    assert_eq!(low.partial, None);
    let mixed = controller.poll().await.unwrap();
    assert_eq!(
        mixed.confirmed, None,
        "one full report cannot confirm a full-flow streak"
    );
    assert_eq!(mixed.partial, Some(Command::Charge { watts: 500 }));
}
