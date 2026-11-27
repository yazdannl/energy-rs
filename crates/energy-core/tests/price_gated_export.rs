//! The hybrid export gate: a self-consumption slot follows the meter to zero unless its feed-in
//! price clears the plan's own cost-basis floor, in which case the plan's setpoint stands.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::dispatch::{
    Dispatcher, FleetView, MeterReading, Overrides, PackReading, TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

const FLOOR: f64 = 0.389_284;
const PAID_FEED: f64 = 0.646_561;
const CHEAP_FEED: f64 = 0.20;
const PLAN_W: f64 = -3_200.0;
const MEASURED_GRID_W: f64 = 1_200.0;

/// A self-consumption slot: the forecast house (3.5 kW) exceeds the planned discharge, so the
/// plan's own grid is positive and the follow used to cap the fleet at the measured load.
fn gated_plan(feed_in: f64, floor: Option<f64>, battery_kw: f64, load_kw: f64) -> Plan {
    let mut plan = plan(NOW, battery_kw, 1.92);
    plan.sell_floor = floor;
    for slot in &mut plan.slots {
        slot.buy_all_in = 0.5;
        slot.feed_in = feed_in;
        slot.load_kw = load_kw;
        slot.pv_kw = 0.0;
    }
    plan
}

fn four_packs() -> Vec<PackReading> {
    (1..=4)
        .map(|index| pack(&format!("SN-{index}"), 25.0))
        .collect()
}

/// One dispatcher tick; `target_w` is the fleet target after the follow and the grid clamps.
async fn tick(plan: &Plan, packs: Vec<PackReading>, grid_w: f64) -> TickOutcome {
    tick_with_meter(
        plan,
        packs,
        MeterReading {
            grid_w: Some(grid_w),
            load_kw: Some(grid_w / 1000.0),
            pv_kw: Some(0.0),
            observed_at: None,
        },
    )
    .await
}

async fn tick_with_meter(plan: &Plan, packs: Vec<PackReading>, meter: MeterReading) -> TickOutcome {
    let fleet = FleetView::new(packs);
    let overrides = Overrides::none();
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    dispatcher.tick(&input, &FakeSink::new()).await
}

async fn target_w(plan: &Plan, grid_w: f64) -> f64 {
    tick(plan, four_packs(), grid_w).await.target_w
}

#[tokio::test]
async fn feed_at_the_floor_keeps_the_full_planned_discharge() {
    let plan = gated_plan(FLOOR, Some(FLOOR), -3.2, 3.5);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, PLAN_W);
}

#[tokio::test]
async fn feed_just_below_the_floor_still_follows_the_grid() {
    // 5e-5 is inside the old netting epsilon: only the ladder's exact comparison catches this.
    let plan = gated_plan(FLOOR - 5e-5, Some(FLOOR), -3.2, 3.5);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, -MEASURED_GRID_W);
}

#[tokio::test]
async fn feed_just_above_the_floor_keeps_the_full_planned_discharge() {
    let plan = gated_plan(FLOOR + 1e-6, Some(FLOOR), -3.2, 3.5);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, PLAN_W);
}

#[tokio::test]
async fn feed_above_the_floor_keeps_the_full_planned_discharge() {
    let plan = gated_plan(PAID_FEED + 0.01, Some(FLOOR), -3.2, 3.5);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, PLAN_W);
}

#[tokio::test]
async fn feed_below_the_floor_still_follows_the_grid_to_zero() {
    let plan = gated_plan(CHEAP_FEED, Some(FLOOR), -3.2, 3.5);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, -MEASURED_GRID_W);
}

#[tokio::test]
async fn a_paid_zero_power_hold_still_follows_the_grid() {
    let plan = gated_plan(FLOOR, Some(FLOOR), 0.0, 0.8);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, -MEASURED_GRID_W);
}

#[tokio::test]
async fn a_paid_pv_charging_slot_still_follows_the_grid() {
    // Self-consumption charging from a PV surplus: battery and planned grid both flow in, so this
    // is not a planned discharge even though the feed-in price clears the floor.
    let mut plan = gated_plan(FLOOR, Some(FLOOR), 0.5, 0.5);
    for slot in &mut plan.slots {
        slot.pv_kw = 1.5;
    }
    let meter = MeterReading {
        grid_w: Some(-1_000.0),
        load_kw: Some(0.5),
        pv_kw: Some(1.5),
        observed_at: None,
    };
    let outcome = tick_with_meter(&plan, four_packs(), meter).await;
    assert_eq!(outcome.target_w, 1_000.0, "absorb the measured surplus");
}

#[tokio::test]
async fn a_missing_sell_floor_still_follows_the_grid_to_zero() {
    let plan = gated_plan(PAID_FEED, None, -3.2, 3.5);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, -MEASURED_GRID_W);
}

#[tokio::test]
async fn non_finite_feed_and_floor_fail_closed_to_zero_grid_follow() {
    for (feed, floor) in [
        (f64::NAN, Some(FLOOR)),
        (f64::INFINITY, Some(FLOOR)),
        (f64::NEG_INFINITY, Some(FLOOR)),
        (PAID_FEED, Some(f64::NAN)),
        (PAID_FEED, Some(f64::INFINITY)),
        (PAID_FEED, Some(f64::NEG_INFINITY)),
    ] {
        let plan = gated_plan(feed, floor, -3.2, 3.5);
        assert_eq!(
            target_w(&plan, MEASURED_GRID_W).await,
            -MEASURED_GRID_W,
            "non-finite prices must not unlock the export gate"
        );
    }
}

#[tokio::test]
async fn a_deliberate_export_is_unchanged_by_the_floor() {
    // Load 0.3 kW against a 3.2 kW discharge: battery and grid both flow out, so the slot's number
    // is a battery rate the follow never touches, whatever the feed price and floor say.
    let plan = gated_plan(CHEAP_FEED, Some(0.9), -3.2, 0.3);
    assert_eq!(target_w(&plan, MEASURED_GRID_W).await, PLAN_W);
}

#[tokio::test]
async fn a_netted_reserve_hold_is_unchanged_by_the_floor() {
    let mut plan = gated_plan(0.415_838, Some(0.9), 0.0, 0.8);
    for slot in &mut plan.slots {
        slot.buy_all_in = 0.415_838;
    }
    assert_eq!(target_w(&plan, 800.0).await, 0.0);
}

#[tokio::test]
async fn a_paid_self_consumption_slot_commands_all_four_packs_to_the_plan() {
    let plan = gated_plan(PAID_FEED, Some(FLOOR), -3.2, 3.5);
    let outcome = tick(&plan, four_packs(), MEASURED_GRID_W).await;
    assert_eq!(outcome.target_w, PLAN_W);
    assert_eq!(outcome.writes(), 4, "every pack was commanded to discharge");
    for id in ["SN-1", "SN-2", "SN-3", "SN-4"] {
        let intended_w = outcome.report(id).expect("pack report").intended_w;
        assert!(
            (intended_w + 800.0).abs() < 1e-6,
            "{id} must deliver its full share of the planned export: {intended_w} W"
        );
    }
}

#[tokio::test]
async fn an_unpaid_self_consumption_slot_does_not_command_the_full_export() {
    let plan = gated_plan(CHEAP_FEED, Some(FLOOR), -3.2, 3.5);
    let outcome = tick(&plan, four_packs(), MEASURED_GRID_W).await;
    assert_eq!(outcome.target_w, -MEASURED_GRID_W, "the meter is followed");
    let commanded: f64 = outcome.packs.iter().map(|pack| pack.intended_w).sum();
    assert!(
        commanded > PLAN_W && (commanded + MEASURED_GRID_W).abs() < 1.0,
        "only the measured load is covered: {commanded} W"
    );
    assert!(
        outcome.packs.iter().any(|pack| pack.intended_w > -800.0),
        "at least one pack stays below its full discharge rate"
    );
}
