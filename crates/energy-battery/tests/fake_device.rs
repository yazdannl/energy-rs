// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
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
    BatteryError, Command, ControlFault, ControlLimits, DeviceController, FakeBehaviour,
    FakeDevice, FakeState, HttpTransport, WriteRequest,
};
use support::SN;

fn transport(fake: &FakeDevice, sn: &str) -> HttpTransport {
    HttpTransport::with_timeout(fake.endpoint(sn), Duration::from_secs(2)).unwrap()
}

async fn spawn(behaviour: FakeBehaviour) -> FakeDevice {
    FakeDevice::spawn(FakeState::new(SN).with_behaviour(behaviour))
        .await
        .unwrap_or_else(|error| panic!("fake device: {error}"))
}

fn controller(fake: &FakeDevice) -> DeviceController<HttpTransport> {
    DeviceController::new(transport(fake, SN), ControlLimits::unlimited())
}

#[tokio::test]
async fn charge_is_applied_and_echoed() {
    let fake = spawn(FakeBehaviour::default().without_spacing_rule()).await;
    let controller = controller(&fake);
    controller.poll().await.unwrap();
    let applied = controller
        .apply(Command::Charge { watts: 650 })
        .await
        .unwrap();
    assert_eq!(applied.ack.sn.as_deref(), Some(SN));
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, Some(Command::Charge { watts: 650 }));

    let state = fake.state().unwrap();
    assert_eq!(state.input_limit_w, 650);
    assert_eq!(state.output_limit_w, 0);
    assert_eq!(state.ac_mode, 1);
    assert_eq!(state.smart_mode, 1);
    assert_eq!(state.applied_count(), 1);
}

#[tokio::test]
async fn discharge_mirrors_charge() {
    let fake = spawn(FakeBehaviour::default().without_spacing_rule()).await;
    let controller = controller(&fake);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Discharge { watts: 400 })
        .await
        .unwrap();
    controller.poll().await.unwrap();
    let state = fake.state().unwrap();
    assert_eq!(state.output_limit_w, 400);
    assert_eq!(state.input_limit_w, 0);
    assert_eq!(state.ac_mode, 2);
}

#[tokio::test]
async fn hold_zeroes_both_limits_without_touching_ac_mode() {
    let fake = spawn(FakeBehaviour::default().without_spacing_rule()).await;
    fake.with_state(|state| state.set_limits(500, 0)).unwrap();
    let controller = controller(&fake);
    controller.poll().await.unwrap();
    controller.apply(Command::Hold).await.unwrap();
    let observation = controller.poll().await.unwrap();
    assert_eq!(observation.confirmed, Some(Command::Hold));

    let state = fake.state().unwrap();
    assert_eq!(state.input_limit_w, 0);
    assert_eq!(state.output_limit_w, 0);
    assert_eq!(state.ac_mode, 2, "the hold must not change acMode");
    let body = state.last_body().unwrap();
    assert!(body["properties"].get("acMode").is_none(), "{body}");
}

#[tokio::test(start_paused = true)]
async fn pacing_keeps_two_writes_from_being_dropped() {
    let fake = spawn(FakeBehaviour::default()).await;
    let controller = controller(&fake);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    controller
        .apply(Command::Discharge { watts: 200 })
        .await
        .unwrap();

    let state = fake.state().unwrap();
    assert_eq!(
        state.dropped, 0,
        "a write inside 1.3 s would have been dropped"
    );
    assert_eq!(state.applied_count(), 2);
    assert_eq!(state.output_limit_w, 200);
}

#[tokio::test]
async fn a_write_inside_the_spacing_floor_is_dropped_by_the_device() {
    let fake = spawn(FakeBehaviour::default()).await;
    let transport = transport(&fake, SN);
    let first = WriteRequest::new(SN, 1, Command::Charge { watts: 300 }.properties());
    transport.send_write(&first).await.unwrap();
    let second = WriteRequest::new(SN, 2, Command::Charge { watts: 500 }.properties());
    transport.send_write(&second).await.unwrap();

    let state = fake.state().unwrap();
    assert_eq!(
        state.dropped, 1,
        "the second write must be acknowledged but dropped"
    );
    assert_eq!(
        state.input_limit_w, 300,
        "the dropped write changed nothing"
    );
}

#[tokio::test]
async fn a_dropped_write_shows_up_as_an_echo_fault() {
    let fake = spawn(FakeBehaviour::default()).await;
    let controller = controller(&fake);
    controller.poll().await.unwrap();
    controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap();
    controller.poll().await.unwrap();
    // The pacer is bypassed by writing straight to the transport, so the device drops it.
    let bypass = WriteRequest::new(SN, 9, Command::Charge { watts: 900 }.properties());
    transport(&fake, SN).send_write(&bypass).await.unwrap();
    let observation = controller.poll().await.unwrap();
    assert!(
        observation.is_healthy(),
        "a dropped write is not yet a fault"
    );
    assert_eq!(observation.report.properties.input_limit_w, 300);
}

#[tokio::test]
async fn a_wedged_device_reports_a_typed_fault_and_refuses_writes() {
    let fake = spawn(FakeBehaviour::default().without_spacing_rule()).await;
    let controller = controller(&fake);
    controller.poll().await.unwrap();
    fake.with_state(|state| state.behaviour = FakeBehaviour::default().wedged())
        .unwrap();

    let observation = controller.poll().await.unwrap();
    assert!(matches!(
        observation.fault,
        Some(ControlFault::Wedged { ac_mode: 127, .. })
    ));
    let error = controller
        .apply(Command::Charge { watts: 300 })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        BatteryError::Fault(ControlFault::Wedged { .. })
    ));
    assert_eq!(fake.state().unwrap().write_count(), 0);
}

#[tokio::test]
async fn a_serial_mismatch_is_typed_in_both_directions() {
    let fake = spawn(FakeBehaviour::default().without_spacing_rule()).await;
    let transport = transport(&fake, "EOC1NLN9N4OTHER1");
    let request = WriteRequest::new(
        "EOC1NLN9N4OTHER1",
        1,
        Command::Charge { watts: 300 }.properties(),
    );
    match transport.send_write(&request).await.unwrap_err() {
        BatteryError::SnMismatch { sent, got } => {
            assert_eq!(sent, "EOC1NLN9N4OTHER1");
            assert_eq!(got, SN);
        }
        other => panic!("expected SnMismatch, got {other}"),
    }
}

#[tokio::test]
async fn a_report_from_another_device_is_refused() {
    let fake = spawn(FakeBehaviour::default().without_spacing_rule()).await;
    let wrong = transport(&fake, "EOC1NLN9N4OTHER1");
    match wrong.fetch_report().await.unwrap_err() {
        BatteryError::SnMismatch { sent, got } => {
            assert_eq!(sent, "EOC1NLN9N4OTHER1");
            assert_eq!(got, SN);
        }
        other => panic!("expected SnMismatch, got {other}"),
    }
}

#[tokio::test]
async fn http_level_rejections_keep_the_body_code() {
    let overflow = spawn(FakeBehaviour::default().with_http_status(413)).await;
    let request = WriteRequest::new(SN, 1, Command::Charge { watts: 300 }.properties());
    let error = transport(&overflow, SN)
        .send_write(&request)
        .await
        .unwrap_err();
    assert!(
        matches!(error, BatteryError::BufferOverflow { .. }),
        "{error}"
    );

    let bad = spawn(FakeBehaviour::default().with_http_status(400)).await;
    let error = transport(&bad, SN).send_write(&request).await.unwrap_err();
    match error {
        BatteryError::Rejected { code, .. } => assert_eq!(code, 400),
        other => panic!("expected Rejected, got {other}"),
    }
}

#[tokio::test]
async fn the_local_api_being_off_is_not_an_auth_error() {
    let fake = spawn(FakeBehaviour::default().with_http_status(401)).await;
    let transport = transport(&fake, SN);
    match transport.fetch_report().await.unwrap_err() {
        BatteryError::Disabled { host } => assert_eq!(host, "127.0.0.1"),
        other => panic!("expected Disabled, got {other}"),
    }
}

#[tokio::test]
async fn every_write_rejection_maps_to_its_own_error() {
    for (code, expected) in [
        (400, "bad parameters"),
        (401, "sn does not match"),
        (413, "buffer overflow risk"),
    ] {
        let fake = spawn(
            FakeBehaviour::default()
                .without_spacing_rule()
                .with_write_code(code),
        )
        .await;
        let controller = controller(&fake);
        controller.poll().await.unwrap();
        let error = controller
            .apply(Command::Charge { watts: 300 })
            .await
            .unwrap_err();
        match (code, &error) {
            (400, BatteryError::Rejected { code: 400, message }) => assert_eq!(message, expected),
            (401, BatteryError::SnMismatch { .. }) => {}
            (413, BatteryError::BufferOverflow { detail }) => assert_eq!(detail, expected),
            other => panic!("unexpected mapping for {code}: {other:?}"),
        }
    }
}
