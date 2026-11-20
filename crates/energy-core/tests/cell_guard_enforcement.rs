//! The cell latch must neutralise a running discharge, not just clamp the next setpoint.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assertions_on_constants
)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use energy_battery::protection::{CellLatch, LatchReason};
use energy_battery::{
    ControlLimits, DeviceController, FakeBehaviour, FakeDevice, FakeState, HttpTransport,
};
use energy_core::dispatch::{
    BatterySink, Dispatcher, FleetView, MeterReading, Overrides, TickInput,
};
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{NOW, Sent, pack, plan};
use support::harness;

fn latched(pack_id: &str, voltage: f64, force_charge: bool, at: i64) -> CellLatch {
    let when = Utc.timestamp_opt(at, 0).single().expect("timestamp");
    CellLatch {
        pack_id: pack_id.to_owned(),
        blocked: true,
        force_charge,
        force_charge_w: force_charge.then_some(800),
        since: when,
        reason: if force_charge {
            LatchReason::BelowCutoff
        } else {
            LatchReason::BelowResume
        },
        reason_at: when,
        voltage_v: Some(voltage),
        last_fresh_at: Some(when),
    }
}

/// A discharge is running, a write fault has armed the backoff, then the cell latch engages.
async fn discharge_with_backoff() -> harness::Harness {
    let mut running = pack("SN-A", 50.0);
    running.power_w = -800.0;
    let h = harness::build(
        Authority::Auto,
        vec![running.clone()],
        plan(NOW, -0.8, 0.96),
    );
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("first tick");
    h.engine.tick_once(NOW + 5).await.expect("discharge");
    assert_eq!(h.sink.writes().last(), Some(&-800.0));

    h.sink.reject_writes(true);
    h.engine.tick_once(NOW + 10).await.expect("failed write");
    h.sink.reject_writes(false);
    assert!(
        h.engine.dispatcher().faults().blocking("SN-A", NOW + 15),
        "the fault backoff should still be armed when the latch engages"
    );
    h
}

#[tokio::test]
async fn a_latched_pack_is_held_while_the_fault_backoff_is_active() {
    let h = discharge_with_backoff().await;
    let mut low = pack("SN-A", 50.0);
    low.power_w = -799.0; // the pack is still discharging when the latch arrives
    low.min_cell_v = Some(3.15); // between the shipped 3.00 cutoff and 3.18 resume
    h.fleet.set(vec![low]);

    let before = h.sink.len();
    let outcome = h.engine.tick_once(NOW + 15).await.expect("latched tick");
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.reason, Reason::CellVoltageGuard);
    assert!(report.written && report.verified, "the hold must be echoed");
    assert_eq!(
        h.sink.commands()[before..].to_vec(),
        vec![("SN-A".to_owned(), Sent::Hold)],
        "the guard must write a hold even while the write path is backing off"
    );

    // And it stays held on the next tick, not just once on the transition.
    let before = h.sink.len();
    h.engine.tick_once(NOW + 20).await.expect("held tick");
    assert_eq!(
        h.sink.commands()[before..].to_vec(),
        vec![("SN-A".to_owned(), Sent::Hold)]
    );
}

#[tokio::test]
async fn clearing_the_latch_resumes_normal_discharge() {
    let h = discharge_with_backoff().await;
    let mut low = pack("SN-A", 50.0);
    low.min_cell_v = Some(3.15);
    h.fleet.set(vec![low]);
    h.engine.tick_once(NOW + 15).await.expect("latched tick");
    assert!(
        h.sink
            .commands()
            .last()
            .is_some_and(|(_, sent)| *sent == Sent::Hold)
    );

    h.fleet.set(vec![pack("SN-A", 50.0)]); // 3.30 V, above resume
    let outcome = h.engine.tick_once(NOW + 25).await.expect("cleared tick");
    assert_eq!(outcome.report("SN-A").expect("report").applied_w, -800.0);
    assert_eq!(h.sink.writes().last(), Some(&-800.0));
}

/// The real controller and echo path: a hold must zero a pack that was delivering 799 W.
#[tokio::test]
async fn a_latch_hold_reaches_the_fake_device_and_is_echoed() {
    let fake = FakeDevice::spawn(
        FakeState::new("SN-A").with_behaviour(FakeBehaviour::default().without_spacing_rule()),
    )
    .await
    .expect("fake device");
    fake.with_state(|state| state.set_limits(0, 799))
        .expect("limits");
    let io = HttpTransport::with_timeout(fake.endpoint("SN-A"), Duration::from_secs(2))
        .expect("transport");
    let controller = Arc::new(DeviceController::new(io, ControlLimits::unlimited()));
    controller.poll().await.expect("prime the controller");

    let mut reading = pack("SN-A", 11.0);
    reading.power_w = -799.0;
    reading.min_cell_v = Some(3.15);
    let view = FleetView::new(vec![reading]);
    let plan = plan(NOW, -0.8, 0.96);
    let meter = MeterReading::default();
    let overrides = Overrides::none();
    let latches = [latched("SN-A", 3.15, false, NOW)];
    let input = TickInput {
        now: NOW + 1,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &view,
        meter: &meter,
        latches: &latches,
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    dispatcher.faults().arm("SN-A", NOW);
    let outcome = dispatcher
        .tick(&input, &BatterySink::new(vec![Arc::clone(&controller)]))
        .await;

    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.reason, Reason::CellVoltageGuard);
    assert!(
        report.written && report.verified,
        "the hold was not confirmed"
    );
    let state = fake.state().expect("device state");
    assert_eq!(state.output_limit_w, 0, "the device is still discharging");
    assert_eq!(state.input_limit_w, 0);
    assert!(
        state.last_body().unwrap()["properties"]
            .get("acMode")
            .is_none()
    );
}

#[tokio::test]
async fn below_cutoff_still_forces_an_emergency_charge() {
    let mut low = pack("SN-A", 11.0);
    low.min_cell_v = Some(2.90);
    let h = harness::build(Authority::Auto, vec![low], plan(NOW, -0.8, 0.96));
    h.engine.boot(NOW).await.expect("boot");
    h.engine.tick_once(NOW).await.expect("tick");

    let outcome = h.engine.tick_once(NOW + 5).await.expect("charged tick");
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(h.sink.writes().last(), Some(&800.0));
    assert!(
        matches!(
            report.reason,
            Reason::CellVoltageEmergencyCharge | Reason::CellVoltageGuard
        ),
        "reason {:?}",
        report.reason
    );
}
