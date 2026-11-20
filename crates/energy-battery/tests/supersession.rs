//! A safety hold replaces a waiter without turning that waiter into a second hold.

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
    BatteryError, Command, ControlLimits, DeviceController, Report, SAFETY_HOLD_WINDOW,
};
use serde_json::json;
use support::broken_reads::BrokenReads;
use support::{SN, ScriptedDevice, report_at};
use tokio::time::Instant;

fn baseline() -> Report {
    report_at(
        SN,
        4211,
        1_791_959_470,
        json!({"acMode": 2, "inputLimit": 0, "outputLimit": 0}),
    )
}

#[tokio::test(start_paused = true)]
async fn a_read_error_after_a_hold_supersedes_the_waiter() {
    let reads = Arc::new(BrokenReads::new(SN));
    reads.start_healthy();
    let controller = Arc::new(
        DeviceController::new(Arc::clone(&reads), ControlLimits::unlimited())
            .with_spacing(Duration::ZERO),
    );
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
    tokio::task::yield_now().await;

    controller.apply(Command::Hold).await.unwrap();
    // Only now does the original waiter's read fail: its command is gone, so this is a supersession.
    reads.break_reads();
    let observation = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(observation.superseded);
    assert!(observation.fault.is_none());
    assert!(observation.confirmed.is_none());
    assert_eq!(controller.pending().unwrap().command, Command::Hold);
}

#[tokio::test(start_paused = true)]
async fn a_hold_lands_when_another_writer_races_its_preemption() {
    let device = Arc::new(ScriptedDevice::new(SN, vec![baseline()]));
    let controller = Arc::new(
        DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
            .with_spacing(Duration::ZERO),
    );
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();

    let other = tokio::spawn({
        let controller = Arc::clone(&controller);
        async move { controller.apply(Command::Charge { watts: 400 }).await }
    });
    let started = Instant::now();
    let hold = tokio::spawn({
        let controller = Arc::clone(&controller);
        async move { controller.apply(Command::Hold).await }
    });
    assert!(matches!(
        other.await.unwrap(),
        Err(BatteryError::Unconfirmed { .. })
    ));
    hold.await.unwrap().unwrap();
    assert!(
        Instant::now().duration_since(started) <= SAFETY_HOLD_WINDOW + Duration::from_millis(50),
        "the hold must not wait past the safety bound"
    );
    assert_eq!(
        device.write_count(),
        2,
        "only the hold followed the original write"
    );
    assert_eq!(controller.pending().unwrap().command, Command::Hold);
}
