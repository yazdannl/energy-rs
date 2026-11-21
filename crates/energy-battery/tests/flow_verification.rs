//! Verification needs real AC flow, not just a matching limit echo.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::time::Duration;

use energy_battery::{
    Command, ControlFault, ControlLimits, DeviceController, FAULT_BACKOFF, FakeBehaviour,
    FakeDevice, FakeState, HttpTransport,
};
use support::SN;

async fn device(behaviour: FakeBehaviour) -> (FakeDevice, DeviceController<HttpTransport>) {
    let fake = FakeDevice::spawn(FakeState::new(SN).with_behaviour(behaviour))
        .await
        .unwrap();
    let transport = HttpTransport::with_timeout(fake.endpoint(SN), Duration::from_secs(2)).unwrap();
    let controller = DeviceController::new(transport, ControlLimits::unlimited());
    controller.poll().await.unwrap();
    (fake, controller)
}

#[tokio::test]
async fn an_echo_with_no_grid_flow_faults_and_is_not_resent() {
    let (fake, controller) = device(FakeBehaviour::default().without_flow()).await;
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
    let (fake, controller) = device(FakeBehaviour::default().without_flow()).await;
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
    assert!(controller.last_fault().is_none());
    assert!(controller.active_fault().is_none());
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
    let (fake, controller) = device(FakeBehaviour::default().without_flow()).await;
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
    let (fake, controller) = device(FakeBehaviour::default().without_flow()).await;
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
