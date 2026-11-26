//! Verification needs real AC flow, not just a matching limit echo.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use serde_json::json;

use energy_battery::{
    Command, ControlFault, ControlLimits, DeviceController, FAULT_BACKOFF, FakeBehaviour,
    FakeDevice, FakeState, HttpTransport,
};
use std::sync::Arc;
use std::time::Duration;
use support::{SN, ScriptedDevice, report_at};

async fn device(behaviour: FakeBehaviour) -> (FakeDevice, DeviceController<HttpTransport>) {
    let fake = FakeDevice::spawn(FakeState::new(SN).with_behaviour(behaviour))
        .await
        .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(2)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    controller.poll().await.unwrap();
    (fake, controller)
}

async fn device_no_window(
    behaviour: FakeBehaviour,
) -> (FakeDevice, DeviceController<HttpTransport>) {
    let (fake, controller) = device(behaviour).await;
    (fake, controller.with_flow_window(Duration::ZERO))
}

#[tokio::test]
async fn an_echo_with_no_grid_flow_faults_and_is_not_resent() {
    let (fake, controller) = device_no_window(FakeBehaviour::default().without_flow()).await;
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_millis(300), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, None);
    assert!(
        matches!(
            observation.fault,
            Some(ControlFault::NoFlow { observed_w, .. }) if observed_w <= 20
        ),
        "the pack echoed 300 W but drew nothing: {observation:?}"
    );
    assert!(controller.pending().is_none());

    // The controller-level backoff outlives the engine's tick cadence: no second write.
    let retry = tokio::time::timeout(Duration::from_millis(100), async {
        controller.apply(Command::Charge { watts: 300 }).await
    })
    .await;
    assert!(retry.is_err(), "a no-flow pack must not be re-commanded");
    assert_eq!(fake.state().unwrap().write_count(), 1);
    assert_eq!(FAULT_BACKOFF.as_secs(), 300);
}

#[tokio::test]
async fn a_message_id_reset_faults_and_leaves_the_pack_alone() {
    let (fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    let confirmed = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(confirmed.confirmed, Some(Command::Charge { watts: 300 }));

    // The local API came back with a fresh, lower counter and a newer clock.
    fake.with_state(|state| {
        state.message_id = 10;
        state.timestamp += 1;
    })
    .unwrap();
    let observation = controller.poll().await.unwrap();
    assert!(
        matches!(observation.fault, Some(ControlFault::Restarted { .. })),
        "the reset must be a typed fault: {observation:?}"
    );

    let retry = tokio::time::timeout(Duration::from_millis(100), async {
        controller.apply(Command::Charge { watts: 300 }).await
    })
    .await;
    assert!(retry.is_err(), "a resetting pack must be left alone");
    assert_eq!(fake.state().unwrap().write_count(), 1);
}

#[tokio::test]
async fn a_charge_with_measured_grid_flow_is_verified() {
    let (fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 300 }));
    assert!(observation.fault.is_none());
    let properties = observation.report.properties;
    assert!(properties.grid_input_w > 0);
    assert_eq!(properties.ac_status, Some(2));
    assert_eq!(properties.dc_status, Some(1));
    assert_eq!(fake.state().unwrap().write_count(), 1);
}

#[tokio::test]
async fn a_discharge_with_measured_home_flow_is_verified() {
    let (_fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Discharge { watts: 300 })
        .await
        .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(
        observation.confirmed,
        Some(Command::Discharge { watts: 300 })
    );
    assert!(observation.fault.is_none());
    let properties = observation.report.properties;
    assert!(properties.output_home_w > 0);
    assert_eq!(properties.ac_status, Some(1));
    assert_eq!(properties.dc_status, Some(2));
}

#[tokio::test]
async fn a_discharge_confirms_on_measured_power_even_if_status_says_charging() {
    let (fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Discharge { watts: 300 })
        .await
        .unwrap();
    // Power is authoritative: even a status actively reporting the other direction cannot block
    // a verified discharge. energy-core raises the mapping alarm for this contradiction.
    fake.with_state(|state| {
        state.ac_status = 2;
        state.dc_status = 1;
    })
    .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(
        observation.confirmed,
        Some(Command::Discharge { watts: 300 })
    );
    assert!(observation.fault.is_none());
    assert!(observation.report.properties.output_home_w > 0);
    assert_eq!(observation.report.properties.ac_status, Some(2));
    assert_eq!(observation.report.properties.dc_status, Some(1));
}

#[tokio::test]
async fn a_low_command_taper_is_insufficient_evidence_not_a_no_flow_fault() {
    assert_eq!(energy_battery::flow_threshold_w(1000), 100);
    assert_eq!(
        energy_battery::flow_threshold_w(45),
        energy_battery::FLOW_THRESHOLD_W
    );
    let (fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Charge { watts: 1000 })
        .await
        .unwrap();
    fake.with_state(|state| {
        state.grid_input_w = 45;
        state.ac_status = 2;
        state.dc_status = 1;
    })
    .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(observation.confirmed, None);
    assert!(
        !matches!(observation.fault, Some(ControlFault::NoFlow { .. })),
        "45 W is real, low output: {observation:?}"
    );
    assert_eq!(
        observation.partial,
        Some(Command::Charge { watts: 1000 }),
        "curtailment is a non-fault outcome, not a failed start: {observation:?}"
    );
    assert!(observation.fault.is_none());
}

#[tokio::test]
async fn idle_telemetry_does_not_erase_a_no_flow_episode() {
    let (fake, controller) = device_no_window(FakeBehaviour::default().without_flow()).await;
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_millis(300), || {})
        .await
        .unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::NoFlow { .. })
    ));
    assert!(matches!(
        controller.last_fault(),
        Some(ControlFault::NoFlow { .. })
    ));

    fake.with_state(|state| {
        state.input_limit_w = 0;
        state.output_limit_w = 0;
        state.grid_input_w = 0;
        state.ac_status = 0;
        state.dc_status = 0;
        state.ac_mode = 1;
    })
    .unwrap();
    for _ in 0..3 {
        assert!(controller.poll().await.unwrap().fault.is_none());
    }
    assert!(
        matches!(controller.last_fault(), Some(ControlFault::NoFlow { .. })),
        "an idle, healthy-looking report is not recovery evidence"
    );
    assert!(matches!(
        controller.active_fault(),
        Some(ControlFault::NoFlow { .. })
    ));
    controller.acknowledge_fault();
    assert!(controller.last_fault().is_none(), "history is acknowledged");
    assert!(
        matches!(controller.active_fault(), Some(ControlFault::NoFlow { .. })),
        "acknowledgement cannot clear an unresolved active episode"
    );
}

#[tokio::test]
async fn a_sustained_low_taper_confirms_partially_without_a_fault() {
    let (fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Charge { watts: 1000 })
        .await
        .unwrap();
    fake.with_state(|state| {
        state.grid_input_w = 45;
        state.ac_status = 2;
        state.dc_status = 1;
    })
    .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(observation.partial, Some(Command::Charge { watts: 1000 }));
    assert_eq!(observation.confirmed, None);
    assert!(observation.fault.is_none());
    assert!(controller.pending().is_none());
    assert_eq!(fake.state().unwrap().write_count(), 1);
    assert!(
        observation.report.properties.grid_input_w == 45,
        "the receipt must come from the 45 W measurement, not the 1000 W echo"
    );
}

#[tokio::test]
async fn one_to_four_watts_is_noise_not_partial_confirmation() {
    let (fake, controller) = device_no_window(FakeBehaviour::default().without_flow()).await;
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    fake.with_state(|state| {
        state.grid_input_w = 4;
        state.ac_status = 2;
        state.dc_status = 1;
    })
    .unwrap();
    let observation = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::NoFlow { observed_w, .. }) if observed_w <= 4
    ));
    assert_eq!(observation.partial, None);
}

#[tokio::test]
async fn five_watts_is_noise_and_six_watts_is_real_but_low_flow() {
    let (fake, controller) = device_no_window(FakeBehaviour::default().without_flow()).await;
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    fake.with_state(|state| {
        state.grid_input_w = 5;
        state.ac_status = 2;
        state.dc_status = 1;
    })
    .unwrap();
    let five = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert!(matches!(five.fault, Some(ControlFault::NoFlow { .. })));
    assert_eq!(five.partial, None);

    let (fake, controller) = device(FakeBehaviour::default()).await;
    controller
        .apply(Command::Charge { watts: 500 })
        .await
        .unwrap();
    fake.with_state(|state| {
        state.grid_input_w = 6;
        state.ac_status = 2;
        state.dc_status = 1;
    })
    .unwrap();
    let six = controller
        .poll_confirming(Duration::from_secs(2), || {})
        .await
        .unwrap();
    assert_eq!(six.partial, Some(Command::Charge { watts: 500 }));
    assert!(six.fault.is_none());
}

#[tokio::test(start_paused = true)]
async fn absent_reports_before_the_window_are_pending_not_partial_flow() {
    let base = 1_700_000_000;
    let echo = |id: u32, ts: i64| {
        report_at(
            SN,
            id,
            ts,
            json!({
                "acMode": 1, "inputLimit": 1000, "outputLimit": 0,
                "acStatus": 0, "dcStatus": 0, "gridInputPower": 0, "faultLevel": 0
            }),
        )
    };
    let device = Arc::new(ScriptedDevice::new(
        SN,
        vec![
            report_at(SN, 1, base, json!({})),
            echo(2, base + 1),
            echo(3, base + 2),
            echo(4, base + 46),
        ],
    ));
    let controller = DeviceController::new(Arc::clone(&device), ControlLimits::unlimited());
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 1000 })
        .await
        .unwrap();

    for _ in 0..2 {
        let seen = controller.poll().await.unwrap();
        assert!(
            seen.confirmed.is_none() && seen.partial.is_none() && seen.fault.is_none(),
            "{seen:?}"
        );
    }
    tokio::time::advance(Duration::from_secs(46)).await;
    let _seen = controller.poll().await.unwrap();
    let pending = controller
        .pending()
        .expect("ownership survives the downgrade");
    assert!(
        matches!(&pending.inconclusive, Some(ControlFault::NoFlow { observed_w, sample_at, write_at, .. })
            if *observed_w <= 5 && *sample_at == base + 46 && *write_at > *sample_at),
        "a zero after the original window is a real failed start: {pending:?}"
    );
    assert!(
        matches!(
            controller.active_fault(),
            Some(ControlFault::UncertainWrite { .. })
        ),
        "skewed clocks escalate to a bounded verified-hold recovery"
    );
}

#[tokio::test(start_paused = true)]
async fn skewed_reports_keep_the_original_deadline_and_ownership() {
    let base = 1_700_000_000;
    let skewed = |id: u32| {
        report_at(
            SN,
            id,
            base - 100,
            json!({
                "acMode": 1, "inputLimit": 1000, "outputLimit": 0,
                "acStatus": 0, "dcStatus": 0, "gridInputPower": 0, "faultLevel": 0
            }),
        )
    };
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
    let deadline = controller.pending().unwrap().flow_deadline;
    let write_at = controller.pending().unwrap().accepted_at;

    for id in 2..6 {
        device.push_report(skewed(id));
        let _ = controller.poll().await.unwrap();
        let pending = controller.pending().expect("ownership is never lost");
        assert_eq!(
            pending.flow_deadline, deadline,
            "the ramp deadline never resets"
        );
        assert_eq!(pending.accepted_at, write_at);
    }
    tokio::time::advance(Duration::from_secs(46)).await;
    let _ = controller.poll().await.unwrap();
    assert_eq!(controller.pending().unwrap().flow_deadline, deadline);
    assert!(
        controller.active_fault().is_some(),
        "unusable evidence escalates after the bound"
    );
}

/// A device clock can lag the write clock while the pack is genuinely charging. Real positive
/// flow in the commanded direction must never be re-labelled as an uncertain write and held.
#[tokio::test(start_paused = true)]
async fn positive_flow_with_skewed_clocks_never_escalates_to_a_hold() {
    let base = 1_700_000_000;
    let flowing = |id: u32, ts: i64| {
        report_at(
            SN,
            id,
            ts,
            json!({
                "acMode": 1, "inputLimit": 1000, "outputLimit": 0,
                "acStatus": 2, "dcStatus": 1, "gridInputPower": 1000, "faultLevel": 0
            }),
        )
    };
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

    // Every sample proves 1000 W charging, but its device clock is only five seconds past the
    // write, far short of the 45 s causality bound. That clock skew must not arm a hold.
    for id in 2..=8 {
        device.push_report(flowing(id, accepted_at + 5));
        let seen = controller.poll().await.unwrap();
        assert!(seen.fault.is_none(), "skewed flow report: {seen:?}");
        assert!(controller.active_fault().is_none(), "no hold escalation");
    }
    tokio::time::advance(Duration::from_secs(46)).await;
    device.push_report(flowing(9, accepted_at + 6));
    let seen = controller.poll().await.unwrap();
    assert!(seen.fault.is_none(), "flow after the window: {seen:?}");
    assert!(
        controller.active_fault().is_none(),
        "valid delivery survives an expired, skewed clock"
    );
    assert_eq!(
        device.write_count(),
        1,
        "no re-command after a false no-flow"
    );
}
