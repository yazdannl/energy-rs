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
use support::fakes::{FakeSink, NOW, Sent, pack, plan};

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

/// A plan whose end-of-slot target leaves room for a 1.6 kW fleet discharge.
fn discharge_plan(soc_kwh: f64) -> Plan {
    let mut planned = charge_plan(-1600.0);
    for slot in &mut planned.slots {
        slot.soc_kwh = soc_kwh;
        slot.target_soc_kwh = soc_kwh - 1.6;
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
    tick_with(
        dispatcher,
        sink,
        fleet,
        planned,
        now,
        &MeterReading::default(),
    )
    .await
}

async fn tick_with(
    dispatcher: &Dispatcher,
    sink: &FakeSink,
    fleet: &[PackReading],
    planned: &Plan,
    now: i64,
    meter: &MeterReading,
) -> TickOutcome {
    let view = FleetView::new(fleet.to_vec());
    let input = TickInput {
        now,
        mode: Authority::Auto,
        plan: Some(planned),
        fleet: &view,
        meter,
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

/// A latched full pack must still receive a discharge command: the ceiling blocks charge only.
#[tokio::test]
async fn a_ceiling_latched_full_pack_can_discharge() {
    let fleet = vec![
        full("SN-FULL"),
        able("SN-A", 97.0),
        able("SN-B", 97.0),
        able("SN-C", 97.0),
    ];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let charge_sink = FakeSink::new();
    tick(&dispatcher, &charge_sink, &fleet, &charge_plan(2400.0), NOW).await;
    assert!(
        charge_sink.commands().iter().all(|(id, _)| id != "SN-FULL"),
        "the charge tick latched the full pack"
    );

    let sink = FakeSink::new();
    let outcome = tick(&dispatcher, &sink, &fleet, &discharge_plan(7.4), NOW + 5).await;
    let row = outcome.report("SN-FULL").expect("full report");
    assert!(
        row.requested_w < 0.0 && row.intended_w < 0.0,
        "requested={} intended={}: a charge latch must not zero a discharge",
        row.requested_w,
        row.intended_w
    );
    assert_ne!(row.reason, Reason::ChargeAtCeiling);
    assert!(!row.capability_skipped);
    assert!(
        row.note.is_none(),
        "no charge note on a discharge row: {:?}",
        row.note
    );
    assert!(
        sink.commands().iter().any(|(id, sent)| {
            id == "SN-FULL" && matches!(sent, Sent::Write(watts) if *watts < 0.0)
        }),
        "the full pack must get a negative write: {:?}",
        sink.commands()
    );
}

/// An import-guarded reversal is a discharge even when the plan asked to charge.
#[tokio::test]
async fn a_full_pack_can_discharge_under_an_import_guarded_command() {
    let fleet = vec![full("SN-FULL")];
    let dispatcher = Dispatcher::new(
        SafetyLimits {
            max_grid_import_w: 7700.0,
            grid_import_margin_w: 200.0,
            ramp_limit_w: 10_000.0,
            ..limits()
        },
        true,
        10,
    );
    let mut planned = charge_plan(500.0);
    for slot in &mut planned.slots {
        slot.buy_all_in = 0.30;
        slot.feed_in = 0.30;
        slot.soc_kwh = 7.4;
        slot.target_soc_kwh = 7.525;
    }
    let meter = MeterReading::grid(7600.0).observed(NOW);
    let sink = FakeSink::new();
    let outcome = tick_with(&dispatcher, &sink, &fleet, &planned, NOW, &meter).await;

    assert_eq!(outcome.target_w, -100.0, "the guard reversed the charge");
    let row = outcome.report("SN-FULL").expect("full report");
    assert!(row.intended_w < 0.0, "the full pack serves the breach");
    assert_eq!(row.reason, Reason::GridImportCeiling);
    assert!(!row.capability_skipped);
    assert!(row.note.is_none());
    assert!(
        sink.commands().iter().any(|(id, sent)| {
            id == "SN-FULL" && matches!(sent, Sent::Write(watts) if *watts < 0.0)
        }),
        "{:?}",
        sink.commands()
    );
}

/// The row names a charge skip only when it actually refused charge; a real discharge row is clean.
#[tokio::test]
async fn charge_skip_rows_are_honest_in_both_directions() {
    let fleet = vec![full("SN-FULL"), able("SN-A", 60.0)];
    let dispatcher = Dispatcher::new(limits(), true, 10);
    let sink = FakeSink::new();
    let charge = tick(&dispatcher, &sink, &fleet, &charge_plan(1600.0), NOW).await;
    let row = charge.report("SN-FULL").expect("charge row");
    assert_eq!(row.requested_w, 0.0);
    assert_eq!(row.intended_w, 0.0);
    assert_eq!(row.reason, Reason::ChargeAtCeiling);
    assert!(
        row.note
            .as_deref()
            .is_some_and(|note| note.contains("ceiling")),
        "the charge row explains the ceiling: {:?}",
        row.note
    );
    assert!(row.capability_skipped);

    let sink = FakeSink::new();
    // Two packs each hold half the fleet's plan energy: 3.7 kWh leaves the full pack room to
    // serve while respecting the plan-target clause of the safety ladder.
    let discharge = tick(&dispatcher, &sink, &fleet, &discharge_plan(3.7), NOW + 5).await;
    let row = discharge.report("SN-FULL").expect("discharge row");
    assert!(
        row.requested_w < 0.0 && row.intended_w < 0.0,
        "requested={} intended={} reason={:?}",
        row.requested_w,
        row.intended_w,
        row.reason
    );
    assert_ne!(row.reason, Reason::ChargeAtCeiling);
    assert!(
        !row.note
            .as_deref()
            .is_some_and(|note| note.contains("ceiling")),
        "the discharge row must not carry a charge note: {:?}",
        row.note
    );
    assert!(!row.capability_skipped);
    assert!(
        sink.commands().iter().any(|(id, sent)| {
            id == "SN-FULL" && matches!(sent, Sent::Write(watts) if *watts < 0.0)
        }),
        "an honest discharge row must have a negative write: {:?}",
        sink.commands()
    );
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
