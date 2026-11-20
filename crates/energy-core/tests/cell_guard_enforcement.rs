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
    Command, ControlLimits, DeviceController, ECHO_CONFIRM_WINDOW, FakeBehaviour, FakeDevice,
    FakeState, HttpTransport, MIN_WRITE_SPACING,
};
use energy_core::dispatch::{
    BatterySink, Dispatcher, FleetView, MeterReading, Overrides, TickInput,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{NOW, Sent, pack, plan};
use support::harness;
use tokio::time::Instant;

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

fn tick_input<'a>(
    now: i64,
    plan: &'a Plan,
    view: &'a FleetView,
    latches: &'a [CellLatch],
    meter: &'a MeterReading,
    overrides: &'a Overrides,
) -> TickInput<'a> {
    TickInput {
        now,
        mode: Authority::Auto,
        plan: Some(plan),
        fleet: view,
        meter,
        latches,
        overrides,
        stale: false,
        progress: None,
        selection: None,
    }
}

fn latched_fixture(voltage: f64) -> (FleetView, Plan, MeterReading, Overrides, [CellLatch; 1]) {
    let mut reading = pack("SN-A", 50.0);
    reading.power_w = -799.0;
    reading.min_cell_v = Some(voltage);
    (
        FleetView::new(vec![reading]),
        plan(NOW, -0.8, 0.96),
        MeterReading::default(),
        Overrides::none(),
        [latched("SN-A", voltage, false, NOW)],
    )
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

/// A real zenSDK fake: default 1.3 s device drop rule, 2 s controller pacer, real ceilings.
async fn production_device(
    behaviour: FakeBehaviour,
) -> (FakeDevice, Arc<DeviceController<HttpTransport>>) {
    let fake = FakeDevice::spawn(FakeState::new("SN-A").with_behaviour(behaviour))
        .await
        .expect("fake device");
    fake.with_state(|state| state.set_limits(0, 799))
        .expect("limits");
    let io = HttpTransport::with_timeout(fake.endpoint("SN-A"), Duration::from_secs(2))
        .expect("transport");
    let controller = Arc::new(DeviceController::new(io, ControlLimits::new(1000, 800)));
    assert_eq!(controller.spacing(), MIN_WRITE_SPACING);
    (fake, controller)
}

/// The real controller and echo path: a hold must zero a pack that was delivering 799 W.
#[tokio::test]
async fn a_latch_hold_reaches_the_fake_device_and_is_echoed() {
    let (fake, controller) = production_device(FakeBehaviour::default()).await;
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
    dispatcher.faults().arm("SN-A", NOW, "scripted fault");
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

/// The emergency charge is safety traffic too: it must not wait out a write backoff.
#[tokio::test]
async fn an_emergency_charge_bypasses_the_write_backoff_after_a_neutral_hold() {
    let h = discharge_with_backoff().await;
    let mut low = pack("SN-A", 50.0);
    low.power_w = -799.0;
    low.min_cell_v = Some(3.15);
    h.fleet.set(vec![low]);
    h.engine.tick_once(NOW + 15).await.expect("neutral hold");

    let mut below = pack("SN-A", 50.0);
    below.min_cell_v = Some(2.90); // now below the 3.00 V cutoff
    below.power_w = 0.0; // the hold has already stopped the output
    h.fleet.set(vec![below]);
    assert!(
        h.engine.dispatcher().faults().blocking("SN-A", NOW + 16),
        "the backoff must still be active for this to prove the exemption"
    );

    let outcome = h.engine.tick_once(NOW + 16).await.expect("charged tick");
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.reason, Reason::CellVoltageEmergencyCharge);
    assert_eq!(h.sink.writes().last(), Some(&800.0));
    assert!(report.written && report.verified);
}

/// A discharge still awaiting its echo is preempted by the cell-safety hold at the bounded window.
///
/// The echo path owns confirmation: a neutralising hold first waits out the safety window, then
/// cancels the unresolved command and lands as the single hold for this episode.
#[tokio::test]
async fn a_pending_discharge_is_preempted_by_a_bounded_safety_hold() {
    let (fake, controller) =
        production_device(FakeBehaviour::default().with_stale_echo_reports(1)).await;
    // The pre-write report must differ by more than the 5 W echo tolerance.
    fake.with_state(|state| state.set_limits(0, 0))
        .expect("old limit");
    controller.poll().await.expect("prime the controller");
    controller
        .apply(Command::Discharge { watts: 800 })
        .await
        .expect("discharge write");
    let stale = controller.poll().await.expect("stale echo poll");
    assert_eq!(stale.confirmed, None);
    assert_eq!(
        controller.pending().map(|pending| pending.command),
        Some(Command::Discharge { watts: 800 })
    );
    assert_eq!(fake.state().expect("device").output_limit_w, 800);

    let (view, plan, meter, overrides, latches) = latched_fixture(3.15);
    let input = tick_input(NOW + 1, &plan, &view, &latches, &meter, &overrides);
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    dispatcher.faults().arm("SN-A", NOW, "scripted fault");
    let held = dispatcher
        .tick(&input, &BatterySink::new(vec![Arc::clone(&controller)]))
        .await;
    let report = held.report("SN-A").expect("report");
    assert_eq!(report.reason, Reason::CellVoltageGuard);
    assert!(
        report.written && report.verified,
        "the preempting hold must be confirmed: {report:?}"
    );
    assert_eq!(fake.state().expect("device").write_count(), 2);
    assert!(
        controller.pending().is_none(),
        "the hold resolved both waiters; no second hold is needed"
    );
    assert_eq!(fake.state().expect("device").output_limit_w, 0);
}

/// A failed hold is bounded by the echo window and the fail-safe, then retried and finally cleared.
#[tokio::test]
async fn a_failed_safety_hold_is_bounded_and_control_recovers_after_the_backoff() {
    let (fake, controller) = production_device(FakeBehaviour::default().ignoring_writes()).await;
    controller.poll().await.expect("prime the controller");

    let (view, plan, meter, overrides, latches) = latched_fixture(3.15);
    let input = tick_input(NOW + 1, &plan, &view, &latches, &meter, &overrides);
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let started = Instant::now();
    let failed = dispatcher
        .tick(&input, &BatterySink::new(vec![Arc::clone(&controller)]))
        .await;
    assert!(
        Instant::now().duration_since(started)
            <= 2 * ECHO_CONFIRM_WINDOW + MIN_WRITE_SPACING + Duration::from_millis(500)
    );
    assert!(!failed.report("SN-A").expect("report").written);
    assert_eq!(
        fake.state().expect("device").write_count(),
        2,
        "one latched hold and one fail-safe hold, not a write loop"
    );
    let scripted_drops = fake.state().expect("device").dropped;
    assert!(controller.pending().is_none());

    fake.with_state(|state| state.behaviour = FakeBehaviour::default())
        .expect("restore the device");
    let retry = TickInput {
        now: NOW + 8,
        ..input
    };
    let held = dispatcher
        .tick(&retry, &BatterySink::new(vec![Arc::clone(&controller)]))
        .await;
    let report = held.report("SN-A").expect("report");
    assert!(
        report.written && report.verified,
        "hold not confirmed: {report:?}"
    );
    assert_eq!(fake.state().expect("device").output_limit_w, 0);
    assert_eq!(fake.state().expect("device").write_count(), 3);

    let mut healthy = pack("SN-A", 50.0);
    healthy.min_cell_v = Some(3.30);
    let clear_view = FleetView::new(vec![healthy]);
    // NOW + 12 is past the 10 s backoff armed by the failed tick.
    let clear = tick_input(NOW + 12, &plan, &clear_view, &[], &meter, &overrides);
    let resumed = dispatcher
        .tick(&clear, &BatterySink::new(vec![Arc::clone(&controller)]))
        .await;
    let report = resumed.report("SN-A").expect("report");
    assert_eq!(report.applied_w, -800.0);
    assert!(report.verified);
    assert_eq!(fake.state().expect("device").output_limit_w, 800);
    assert_eq!(
        fake.state().expect("device").dropped,
        scripted_drops,
        "the 2 s pacer let a write through too early"
    );
}
