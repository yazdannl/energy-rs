//! Recovery evidence: pre-write flow cannot arm a hold, and genuine silence still faults.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use energy_battery::control::flow::FLOW_CONFIRM_WINDOW;
use energy_battery::{Command, ControlLimits, DeviceController, Report};
use support::{SN, ScriptedDevice, report_at};

fn flowing_report(id: u32, ts: i64) -> Report {
    report_at(
        SN,
        id,
        ts,
        json!({
            "acMode": 1, "inputLimit": 0, "outputLimit": 0,
            "acStatus": 2, "dcStatus": 1, "gridInputPower": 1000, "faultLevel": 0
        }),
    )
}

fn silent_report(id: u32, ts: i64) -> Report {
    report_at(
        SN,
        id,
        ts,
        json!({
            "acMode": 1, "inputLimit": 1000, "outputLimit": 0,
            "acStatus": 0, "dcStatus": 0, "gridInputPower": 0, "faultLevel": 0
        }),
    )
}

/// The original 04:45Z bug judged flow from a sample taken before the command existed. A sustained
/// 1000 W charge with a stale echo but real flow at accepted_at - 1..-6 must never be held.
#[tokio::test(start_paused = true)]
async fn pre_write_flowing_samples_never_escalate_to_a_hold() {
    // Baseline is 20 s behind the device clock at apply time, so a 6 s lag stays after baseline.
    let base = chrono::Utc::now().timestamp() - 20;
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(SN, 1, base, json!({}))],
    ));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 1000 })
        .await
        .unwrap();
    let accepted_at = controller.pending().expect("command accepted").accepted_at;
    assert!(base < accepted_at - 6, "a 6 s lag must stay after baseline");
    let samples: Vec<i64> = (0..48)
        .map(|tick| accepted_at - 1 - i64::from(tick % 6))
        .collect();
    assert!(
        samples.iter().all(|ts| *ts < accepted_at),
        "every sample must predate the write"
    );
    for (tick, ts) in samples.into_iter().enumerate() {
        device.push_report(flowing_report(2 + tick as u32, ts));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    for _ in 0..46 {
        tokio::time::advance(Duration::from_secs(1)).await;
        let seen = controller.poll().await.unwrap();
        assert!(
            seen.report.timestamp < accepted_at,
            "every consumed sample is pre-write"
        );
        assert!(seen.fault.is_none(), "pre-write flow report: {seen:?}");
        assert!(controller.active_fault().is_none(), "no hold escalation");
    }
    assert!(
        tokio::time::Instant::now() >= deadline,
        "the test crossed the real recovery deadline"
    );
    assert_eq!(
        device.write_count(),
        1,
        "no re-command after a false no-flow"
    );
}

/// A dead pack faults within 45 s plus two 1 s polls, then stays owned by backoff.
#[tokio::test(start_paused = true)]
async fn a_dead_pack_faults_within_the_original_window_and_stays_owned() {
    let base = chrono::Utc::now().timestamp();
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![report_at(SN, 1, base, json!({}))],
    ));
    for id in 2..=60 {
        device.push_report(silent_report(id, base + i64::from(id)));
    }
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 1000 })
        .await
        .unwrap();
    let write_at = device.write_times()[0];

    // Bound: 45 s + two 1 s polls — one records the terminal sample, the next reaps it.
    let bound = FLOW_CONFIRM_WINDOW + Duration::from_secs(2);
    let mut faulted = false;
    let mut last = None;
    for _ in 0..48 {
        tokio::time::advance(Duration::from_secs(1)).await;
        let seen = controller.poll().await.unwrap();
        let now = seen.fault.is_some() || controller.active_fault().is_some();
        last = Some(seen);
        if now {
            faulted = true;
            break;
        }
    }
    assert!(faulted, "dead pack faulted: {last:?}");
    let elapsed = tokio::time::Instant::now().duration_since(write_at);
    assert!(elapsed <= bound, "polling exceeded {bound:?}: {elapsed:?}");
    assert!(
        controller.active_fault().is_some(),
        "a typed fault is active"
    );
    let retry = tokio::time::timeout(Duration::from_millis(100), async {
        controller.apply(Command::Charge { watts: 1000 }).await
    })
    .await;
    assert!(retry.is_err(), "the faulted command must not be renewed");
    assert_eq!(device.write_count(), 1, "no second write during backoff");
}
