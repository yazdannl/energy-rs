//! Replan triggers: no plan, stale plan, expiry, input change, SoC drift and slot boundary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::replan::{Changes, LiveState, PowerMove, ReplanReason, ReplanTrigger};
use support::{NOW, plan, plan_slot};

fn live(energy_kwh: f64) -> LiveState {
    LiveState {
        energy_kwh,
        capacity_kwh: 10.0,
    }
}

fn reason(now: i64, at: i64, energy: f64, changes: Changes) -> Option<ReplanReason> {
    let trigger = ReplanTrigger::default();
    trigger.reason(
        now,
        Some(&plan(at, 5.0)),
        &live(energy),
        &changes,
        &PowerMove::none(),
    )
}

#[test]
fn no_plan_requests_a_replan() {
    let trigger = ReplanTrigger::default();
    assert_eq!(
        trigger.reason(NOW, None, &live(5.0), &Changes::none(), &PowerMove::none()),
        Some(ReplanReason::NoPlan)
    );
}

#[test]
fn a_stale_plan_holds_even_across_a_slot_boundary() {
    assert_eq!(
        reason(NOW + 10_000, NOW, 5.0, Changes::none()),
        Some(ReplanReason::StaleInput)
    );
}

#[test]
fn a_crossed_slot_boundary_requests_a_replan() {
    assert_eq!(
        reason(NOW + 900, NOW, 5.0, Changes::none()),
        Some(ReplanReason::SlotBoundary)
    );
}

#[test]
fn a_three_percent_soc_deviation_requests_a_replan() {
    assert_eq!(
        reason(NOW + 60, NOW, 5.4, Changes::none()),
        Some(ReplanReason::SocDeviation)
    );
    assert_eq!(reason(NOW + 60, NOW, 5.1, Changes::none()), None);
}

#[test]
fn price_and_forecast_changes_request_a_replan() {
    let prices = Changes {
        prices: true,
        forecast: false,
    };
    assert_eq!(
        reason(NOW + 60, NOW, 5.0, prices),
        Some(ReplanReason::PriceChanged)
    );
    let forecast = Changes {
        prices: false,
        forecast: true,
    };
    assert_eq!(
        reason(NOW + 60, NOW, 5.0, forecast),
        Some(ReplanReason::ForecastChanged)
    );
}

#[test]
fn an_expired_horizon_requests_a_replan_before_the_stale_bound() {
    let trigger = ReplanTrigger {
        stale_plan_seconds: 200_000,
        ..ReplanTrigger::default()
    };
    let at = NOW;
    let plan = plan(at, 5.0);
    let now = plan.horizon_end;
    assert_eq!(
        trigger.reason(
            now,
            Some(&plan),
            &live(5.0),
            &Changes::none(),
            &PowerMove::none()
        ),
        Some(ReplanReason::PlanExpired)
    );
}

#[test]
fn a_valid_plan_requests_nothing() {
    assert_eq!(reason(NOW + 60, NOW, 5.0, Changes::none()), None);
}

fn moved(grid_w: Option<f64>, plan_grid_w: Option<f64>) -> PowerMove {
    PowerMove {
        grid_w,
        plan_grid_w,
        pv_w: None,
        plan_pv_w: None,
    }
}

#[test]
fn a_step_at_the_threshold_replans() {
    let trigger = ReplanTrigger::default();
    let power = moved(Some(310.0), Some(300.0));
    assert_eq!(
        trigger.reason(
            NOW,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none(),
            &power
        ),
        Some(ReplanReason::PowerChanged)
    );
}

#[test]
fn a_nine_watt_grid_step_does_not() {
    let trigger = ReplanTrigger::default();
    let power = moved(Some(309.0), Some(300.0));
    assert_ne!(
        trigger.reason(
            NOW,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none(),
            &power
        ),
        Some(ReplanReason::PowerChanged)
    );
}

#[test]
fn a_pv_step_replans_even_when_the_grid_barely_moves() {
    let trigger = ReplanTrigger::default();
    let power = PowerMove {
        grid_w: Some(0.0),
        plan_grid_w: Some(0.0),
        pv_w: Some(900.0),
        plan_pv_w: Some(300.0),
    };
    assert_eq!(
        trigger.reason(
            NOW,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none(),
            &power
        ),
        Some(ReplanReason::PowerChanged)
    );
}

#[test]
fn a_missing_measurement_keeps_the_plan() {
    let trigger = ReplanTrigger::default();
    let power = moved(None, Some(300.0));
    assert_ne!(
        trigger.reason(
            NOW,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none(),
            &power
        ),
        Some(ReplanReason::PowerChanged)
    );
}

#[test]
fn a_forecast_change_wins_over_a_power_move() {
    let trigger = ReplanTrigger::default();
    let power = moved(Some(2000.0), Some(300.0));
    let changes = Changes {
        prices: false,
        forecast: true,
    };
    assert_eq!(
        trigger.reason(NOW, Some(&plan(NOW, 5.0)), &live(5.0), &changes, &power),
        Some(ReplanReason::ForecastChanged)
    );
}

#[test]
fn the_threshold_is_configurable() {
    let trigger = ReplanTrigger {
        grid_react_w: 500.0,
        ..ReplanTrigger::default()
    };
    let power = moved(Some(400.0), Some(300.0));
    assert_ne!(
        trigger.reason(
            NOW,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none(),
            &power
        ),
        Some(ReplanReason::PowerChanged)
    );
    let power = moved(Some(900.0), Some(300.0));
    assert_eq!(
        trigger.reason(
            NOW,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none(),
            &power
        ),
        Some(ReplanReason::PowerChanged)
    );
}

#[test]
fn the_expected_grid_is_the_slot_arithmetic() {
    use energy_core::dispatch::MeterReading;

    // load 2 kW, PV 1.5 kW, battery charging 0.3 kW → grid should import 0.2 kW.
    let mut slot = plan_slot(NOW, 0.3, 5.0);
    slot.load_kw = 2.0;
    slot.pv_kw = 1.5;
    let meter = MeterReading {
        grid_w: Some(250.0),
        load_kw: Some(2.0),
        pv_kw: Some(1.5),
    };
    let power = PowerMove::against(&slot, &meter);
    assert_eq!(power.plan_grid_w, Some(200.0));
    assert_eq!(power.grid_moved_by(), Some(50.0));
    assert_eq!(power.plan_pv_w, Some(1500.0));
    assert_eq!(power.pv_moved_by(), Some(0.0));
    assert!(power.crossed(10.0, 10.0));
    assert!(!power.crossed(200.0, 10.0), "a 50 W move is under 200 W");
}
