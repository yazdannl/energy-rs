//! Minimal device-report charge ceiling: zero closed packs, keep stale behaviour unchanged.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::assert_is_empty
)]

mod support;

use energy_battery::SocLimit;
use energy_core::allocation::{ALLOCATION_DEADBAND_W, Allocation, ChargeSkip, SplitInput};
use energy_core::dispatch::{
    Dispatcher, FleetView, MeterReading, Overrides, PackReading, TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

const EPS_W: f64 = 0.5;

fn full(id: &str) -> PackReading {
    let mut reading = pack(id, 100.0);
    reading.spec.max_soc_pct = 100.0;
    reading.device_soc_pct = Some(100.0);
    reading.soc_limit = SocLimit::Upper;
    reading.fault_level = Some(2);
    reading.energy_kwh = Some(reading.spec.capacity_kwh);
    reading
}

fn able(id: &str, soc_pct: f64) -> PackReading {
    let mut reading = pack(id, soc_pct);
    reading.spec.max_soc_pct = 100.0;
    reading.device_soc_pct = Some(soc_pct);
    reading.energy_kwh = Some(reading.spec.capacity_kwh * soc_pct / 100.0);
    reading
}

fn charge_plan(watts: f64) -> Plan {
    let mut planned = plan(NOW, watts / 1000.0, 20.0);
    for slot in &mut planned.slots {
        slot.soc_kwh = 20.0;
    }
    planned
}

fn limits() -> SafetyLimits {
    SafetyLimits {
        ramp_limit_w: 10_000.0,
        ..SafetyLimits::default()
    }
}

async fn tick(
    dispatcher: &Dispatcher,
    sink: &FakeSink,
    fleet: &[PackReading],
    planned: &Plan,
    now: i64,
) -> TickOutcome {
    let view = FleetView::new(fleet.to_vec());
    let input = TickInput {
        now,
        mode: Authority::Auto,
        plan: Some(planned),
        fleet: &view,
        meter: &MeterReading::default(),
        latches: &[],
        overrides: &Overrides::none(),
        stale: false,
        progress: None,
        selection: None,
    };
    dispatcher.tick(&input, sink).await
}

/// A fresh device-closed pack gets zero and is never written or held; the others keep their share.
#[tokio::test]
async fn a_fresh_full_pack_is_zeroed_and_never_written() {
    let fleet = vec![
        full("SN-FULL"),
        able("SN-A", 50.0),
        able("SN-B", 60.0),
        able("SN-C", 70.0),
    ];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let sink = FakeSink::new();
    let outcome = tick(&dispatcher, &sink, &fleet, &charge_plan(2400.0), NOW).await;

    let report = outcome.report("SN-FULL").expect("full report");
    assert_eq!(report.intended_w, 0.0);
    assert!(report.capability_skipped);
    assert_eq!(report.reason, Reason::ChargeAtCeiling);
    assert_eq!(sink.writes().len(), 3);
    assert!(
        sink.commands().iter().all(|(id, _)| id != "SN-FULL"),
        "no command or hold was sent to the closed pack"
    );
    for id in ["SN-A", "SN-B", "SN-C"] {
        assert!(
            (outcome.report(id).expect("able report").intended_w - 800.0).abs() < 1.0,
            "{id} kept its planned command"
        );
    }
}

/// A fresh 99 % report with no latch is not a ceiling case.
#[tokio::test]
async fn a_fresh_99_percent_pack_is_not_latched() {
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let sink = FakeSink::new();
    let mut almost = able("SN-A", 99.0);
    almost.soc_limit = SocLimit::None;
    let outcome = tick(&dispatcher, &sink, &[almost], &charge_plan(800.0), NOW).await;
    assert!(!outcome.report("SN-A").expect("report").capability_skipped);
    assert_eq!(sink.writes(), vec![800.0]);
}

/// A stale pack behaves exactly as 0.15.12: no skip, no reservation, no target subtraction.
#[tokio::test]
async fn a_stale_pack_has_no_reservation_and_no_target_subtraction() {
    let mut stale = able("SN-STALE", 50.0);
    stale.stale = true;
    stale.power_w = 900.0;
    let fresh = able("SN-A", 50.0);
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let sink = FakeSink::new();
    let outcome = tick(
        &dispatcher,
        &sink,
        &[stale, fresh],
        &charge_plan(1000.0),
        NOW,
    )
    .await;

    assert_eq!(outcome.capability_shortfall_w, 0.0);
    assert!(outcome.capability_skipped.is_empty());
    assert!(
        !outcome
            .report("SN-STALE")
            .expect("stale report")
            .capability_skipped
    );
    assert!(
        (outcome.report("SN-A").expect("fresh report").intended_w - 800.0).abs() < EPS_W,
        "the stale pack's 900 W was not subtracted from the target"
    );
}

/// A closed pack's lost share inside the deadband still produces a typed shortfall.
#[tokio::test]
async fn a_sub_deadband_closed_pack_loss_reports_a_shortfall() {
    let mut closed = able("SN-CLOSED", 99.995);
    closed.soc_limit = SocLimit::Upper;
    let skip = ChargeSkip::new("SN-CLOSED", Reason::ChargeAtCeiling, "device closed");
    let input = SplitInput {
        charge_skips: &[skip],
        ..SplitInput::default()
    };
    let states: Vec<_> = [closed, able("SN-A", 50.0)]
        .iter()
        .filter_map(PackReading::state)
        .collect();
    let allocation = Allocation::split(20.0, &states, &input);
    assert!(
        allocation.capability_shortfall_w > 0.0
            && allocation.capability_shortfall_w < ALLOCATION_DEADBAND_W,
        "loss {:?} must be typed",
        allocation.capability_shortfall_w
    );
    assert_eq!(allocation.unmet_w, 0.0, "dispatch deviation is tolerated");
}

/// An existing latch survives reports with no usable SoC; only a fresh 97 % report releases it.
#[tokio::test]
async fn an_existing_latch_survives_missing_soc_then_releases_at_97() {
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let sink = FakeSink::new();
    let planned = charge_plan(800.0);
    tick(&dispatcher, &sink, &[full("SN-A")], &planned, NOW).await;

    let mut no_soc = full("SN-A");
    no_soc.device_soc_pct = None;
    no_soc.soc_pct = None;
    let outcome = tick(&dispatcher, &sink, &[no_soc], &planned, NOW + 5).await;
    assert!(outcome.report("SN-A").expect("no soc").capability_skipped);

    let mut non_finite = full("SN-A");
    non_finite.device_soc_pct = Some(f64::NAN);
    non_finite.soc_pct = Some(f64::NAN);
    let outcome = tick(&dispatcher, &sink, &[non_finite], &planned, NOW + 10).await;
    assert!(outcome.report("SN-A").expect("nan soc").capability_skipped);

    let mut released = able("SN-A", 97.0);
    released.device_soc_pct = Some(97.0);
    let outcome = tick(&dispatcher, &sink, &[released], &planned, NOW + 15).await;
    assert!(!outcome.report("SN-A").expect("released").capability_skipped);
    assert_eq!(sink.writes(), vec![800.0]);
}

/// Once the remaining packs are saturated, replans keep the same commands and shortfall.
#[tokio::test]
async fn saturated_remaining_packs_do_not_thrash_across_replans() {
    let mut closed = full("SN-FULL");
    closed.soc_limit = SocLimit::Upper;
    let mut a = able("SN-A", 50.0);
    a.spec.max_charge_w = 800.0;
    let mut b = able("SN-B", 60.0);
    b.spec.max_charge_w = 800.0;
    let fleet = [closed, a, b];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let sink = FakeSink::new();
    let planned = charge_plan(2400.0);

    let first = tick(&dispatcher, &sink, &fleet, &planned, NOW).await;
    let second = tick(&dispatcher, &sink, &fleet, &planned, NOW + 5).await;
    assert_eq!(first.capability_shortfall_w, 800.0);
    assert_eq!(first.capability_shortfall_w, second.capability_shortfall_w);
    assert_eq!(sink.writes(), vec![800.0, 800.0, 800.0, 800.0]);
    assert_eq!(sink.holds(), 0, "no holds to the closed pack");
    assert!(first.capability_skipped.iter().all(|id| id == "SN-FULL"));
}

/// Wiring-deferred: the plan generator that would consume the shortfall lives in `energy-decide`,
/// not this crate. `FakeSource` returns a fixed plan, so a test here cannot honestly claim that a
/// shortfall feeds back into the *next generated* plan. The dispatch-side stability across replans
/// is covered by `saturated_remaining_packs_do_not_thrash_across_replans`.
#[test]
#[ignore = "wiring-deferred: shortfall-to-plan feedback belongs to energy-decide"]
fn shortfall_feedback_into_the_next_generated_plan_is_wiring_deferred() {
    panic!("wire the shortfall through energy-decide before enabling this test");
}
