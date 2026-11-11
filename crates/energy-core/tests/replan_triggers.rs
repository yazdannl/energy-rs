//! Replan triggers: no plan, stale plan, expiry, input change, SoC drift and slot boundary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::replan::{Changes, LiveState, ReplanReason, ReplanTrigger};
use support::{NOW, plan};

fn live(energy_kwh: f64) -> LiveState {
    LiveState {
        energy_kwh,
        capacity_kwh: 10.0,
    }
}

fn reason(now: i64, at: i64, energy: f64, changes: Changes) -> Option<ReplanReason> {
    let trigger = ReplanTrigger::default();
    trigger.reason(now, Some(&plan(at, 5.0)), &live(energy), &changes)
}

#[test]
fn no_plan_requests_a_replan() {
    let trigger = ReplanTrigger::default();
    assert_eq!(
        trigger.reason(NOW, None, &live(5.0), &Changes::none()),
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
    // A refresh interval longer than a slot, so the boundary is what fires.
    let trigger = ReplanTrigger {
        replan_seconds: 3600,
        ..ReplanTrigger::default()
    };
    assert_eq!(
        trigger.reason(
            NOW + 900,
            Some(&plan(NOW, 5.0)),
            &live(5.0),
            &Changes::none()
        ),
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
        trigger.reason(now, Some(&plan), &live(5.0), &Changes::none()),
        Some(ReplanReason::PlanExpired)
    );
}

#[test]
fn a_valid_plan_requests_nothing() {
    assert_eq!(reason(NOW + 60, NOW, 5.0, Changes::none()), None);
}

#[test]
fn the_plan_is_refreshed_on_its_own_interval() {
    let trigger = ReplanTrigger::default();
    let plan = plan(NOW, 5.0);
    // Inside the interval, with no other change, nothing asks for a solve.
    assert_eq!(
        trigger.reason(NOW + 60, Some(&plan), &live(5.0), &Changes::none()),
        None
    );
    // At the interval the plan is refreshed even though nothing measurable moved.
    assert_eq!(
        trigger.reason(
            NOW + trigger.replan_seconds,
            Some(&plan),
            &live(5.0),
            &Changes::none()
        ),
        Some(ReplanReason::Due)
    );
}

#[test]
fn the_refresh_interval_is_configurable() {
    let trigger = ReplanTrigger {
        replan_seconds: 120,
        ..ReplanTrigger::default()
    };
    let plan = plan(NOW, 5.0);
    assert_eq!(
        trigger.reason(NOW + 119, Some(&plan), &live(5.0), &Changes::none()),
        None
    );
    assert_eq!(
        trigger.reason(NOW + 120, Some(&plan), &live(5.0), &Changes::none()),
        Some(ReplanReason::Due)
    );
}
