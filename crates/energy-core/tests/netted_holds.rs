//! Dispatch-level regressions: a netted zero setpoint must reach the sink as a hold, while the
//! ladder and the fleet/site clamps keep their authority over every real setpoint.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use chrono::{TimeZone, Utc};
use energy_battery::protection::{CellLatch, LatchReason};
use energy_core::dispatch::{
    Dispatcher, FleetView, MeterReading, Overrides, PackReading, TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

fn netted_plan(battery_kw: f64, load_kw: f64) -> Plan {
    let mut plan = plan(NOW, battery_kw, 0.96);
    for slot in &mut plan.slots {
        slot.buy_all_in = 0.415_838;
        slot.feed_in = 0.415_838;
        slot.load_kw = load_kw;
        slot.pv_kw = 0.0;
    }
    plan
}

fn latch(pack_id: &str, force_w: Option<u32>) -> CellLatch {
    let at = Utc.timestamp_opt(0, 0).single().expect("epoch");
    CellLatch {
        pack_id: pack_id.to_owned(),
        blocked: true,
        force_charge: force_w.is_some(),
        force_charge_w: force_w,
        since: at,
        reason: LatchReason::BelowCutoff,
        reason_at: at,
        voltage_v: Some(3.15),
        last_fresh_at: Some(at),
    }
}

fn limits() -> SafetyLimits {
    SafetyLimits {
        reserve_soc_pct: 10.0,
        ..SafetyLimits::default()
    }
}

async fn tick(
    packs: Vec<PackReading>,
    latches: Vec<CellLatch>,
    limits: SafetyLimits,
    plan: &Plan,
    grid_w: f64,
) -> (TickOutcome, FakeSink) {
    let fleet = FleetView::new(packs);
    let meter = MeterReading::grid(grid_w);
    let overrides = Overrides::none();
    let sink = FakeSink::new();
    let dispatcher = Dispatcher::new(limits, true, 10);
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(plan),
        fleet: &fleet,
        meter: &meter,
        latches: &latches,
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let outcome = dispatcher.tick(&input, &sink).await;
    (outcome, sink)
}

#[tokio::test]
async fn netted_hold_sends_no_discharge_command_to_any_pack() {
    let plan = netted_plan(0.0, 0.8);
    let (outcome, sink) = tick(
        vec![pack("SN-A", 25.0), pack("SN-B", 25.0)],
        Vec::new(),
        limits(),
        &plan,
        800.0,
    )
    .await;
    assert_eq!(outcome.target_w, 0.0, "no grid-follow correction");
    assert!(sink.writes().is_empty(), "nothing was commanded to a pack");
    assert_eq!(sink.holds(), 2, "both packs got a neutral hold");
}

#[tokio::test]
async fn netted_hold_honours_cell_and_soc_floors_and_keeps_emergency_charge() {
    let plan = netted_plan(0.0, 0.8);
    let packs = vec![pack("SN-A", 10.0), pack("SN-B", 10.0)];

    // A blocked cell latch and a pack sitting exactly on the 10 % reserve both stay quiet.
    let (outcome, sink) = tick(
        packs.clone(),
        vec![latch("SN-A", None)],
        limits(),
        &plan,
        800.0,
    )
    .await;
    assert!(sink.writes().is_empty(), "the guard must not be provoked");
    assert_eq!(outcome.report("SN-A").expect("report").intended_w, 0.0);
    assert_eq!(outcome.report("SN-B").expect("report").intended_w, 0.0);

    // Control: with a genuine spread the follow asks for a discharge again, and the SoC floor
    // still blocks it. Skipping the follow did not remove the ladder.
    let mut unnetted = netted_plan(0.0, 0.8);
    for slot in &mut unnetted.slots {
        slot.feed_in = 0.08;
    }
    let (outcome, sink) = tick(packs.clone(), Vec::new(), limits(), &unnetted, 800.0).await;
    assert_eq!(outcome.target_w, -800.0, "a real spread still follows");
    assert!(sink.writes().is_empty(), "the reserve floor still binds");

    // A forcing latch is an emergency charge, unaffected by skipping the follow.
    let (outcome, sink) = tick(
        packs,
        vec![latch("SN-A", Some(500))],
        limits(),
        &plan,
        800.0,
    )
    .await;
    assert_eq!(sink.writes(), vec![500.0], "forced charge stays intact");
    assert_eq!(
        outcome.report("SN-A").expect("report").intended_reason,
        Reason::CellVoltageEmergencyCharge
    );
}

#[tokio::test]
async fn netted_hold_still_respects_the_grid_import_limit() {
    let plan = netted_plan(0.0, 0.0);
    let latches = ["SN-A", "SN-B"]
        .iter()
        .map(|id| latch(id, Some(700)))
        .collect();
    let limits = SafetyLimits {
        max_grid_import_w: 1_000.0,
        ..limits()
    };
    let (outcome, sink) = tick(
        vec![pack("SN-A", 10.0), pack("SN-B", 10.0)],
        latches,
        limits,
        &plan,
        0.0,
    )
    .await;
    assert_eq!(outcome.fleet_clamp, Some(Reason::GridImportLimit));
    let written: f64 = sink.writes().iter().sum();
    assert!(
        (written - 1_000.0).abs() < 5.0,
        "the site limit still binds: {written} W"
    );
}

#[tokio::test]
async fn a_degraded_zero_setpoint_is_held_like_a_hold_under_netting() {
    // Intended behaviour: a zero setpoint is safe whatever put it there. A degraded plan is not a
    // reason to resume self-consumption when import and export settle at the same price.
    let mut plan = netted_plan(0.0, 0.8);
    plan.degraded = Some("degraded forecast".to_owned());
    let (_outcome, sink) = tick(
        vec![pack("SN-A", 25.0), pack("SN-B", 25.0)],
        Vec::new(),
        limits(),
        &plan,
        800.0,
    )
    .await;
    assert!(sink.writes().is_empty(), "a degraded zero is still a hold");
    assert_eq!(sink.holds(), 2);
}
