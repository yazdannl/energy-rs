// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use energy_battery::{FullChargeAction, FullChargeScheduler, FullChargeSettings};

fn utc(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
}

fn weekly() -> FullChargeSettings {
    FullChargeSettings::weekly(1000)
}

fn charge_limit(action: FullChargeAction) -> u32 {
    match action {
        FullChargeAction::Charge { limit_w, .. } => limit_w,
        other => panic!("expected a charge, got {other:?}"),
    }
}

#[test]
fn disabled_maintenance_never_runs() {
    let settings = FullChargeSettings::default();
    let mut scheduler = FullChargeScheduler::new(settings, utc(2026, 3, 23, 12));
    assert!(scheduler.next_run().is_none());
    assert_eq!(
        scheduler.evaluate(utc(2026, 3, 23, 12), Some(50.0), 1000),
        FullChargeAction::Idle
    );
    assert!(!scheduler.is_running());
}

#[test]
fn a_manual_request_charges_immediately() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    assert_eq!(
        scheduler.evaluate(now, Some(50.0), 1000),
        FullChargeAction::Idle
    );
    scheduler.request_manual();
    let action = scheduler.evaluate(now, Some(50.0), 1000);
    assert_eq!(
        action,
        FullChargeAction::Charge {
            limit_w: 1000,
            target_soc_pct: 100.0
        }
    );
    assert_eq!(scheduler.last_run(), Some(now));
    assert!(scheduler.is_running());
}

#[test]
fn the_callers_ceiling_wins_over_the_setting() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    scheduler.request_manual();
    assert_eq!(charge_limit(scheduler.evaluate(now, Some(10.0), 400)), 400);
}

#[test]
fn the_target_is_held_then_released_once() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    scheduler.request_manual();
    scheduler.evaluate(now, Some(50.0), 1000);
    let reached_at = now + Duration::minutes(30);
    let hold = scheduler.evaluate(reached_at, Some(99.5), 1000);
    let until = match hold {
        FullChargeAction::Hold { until, .. } => until,
        other => panic!("expected a hold, got {other:?}"),
    };
    assert_eq!(until, reached_at + Duration::hours(6));
    assert_eq!(
        scheduler.evaluate(until, Some(100.0), 1000),
        FullChargeAction::Release
    );
    assert_eq!(
        scheduler.evaluate(until + Duration::seconds(1), Some(100.0), 1000),
        FullChargeAction::Idle
    );
    assert!(!scheduler.is_running());
}

#[test]
fn slipping_off_the_target_resumes_charging() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    scheduler.request_manual();
    scheduler.evaluate(now, Some(50.0), 1000);
    let first = scheduler.evaluate(now + Duration::minutes(10), Some(100.0), 1000);
    assert!(matches!(first, FullChargeAction::Hold { .. }));
    let slipped = scheduler.evaluate(now + Duration::minutes(20), Some(80.0), 1000);
    assert_eq!(
        slipped,
        FullChargeAction::Charge {
            limit_w: 1000,
            target_soc_pct: 100.0
        }
    );
    let again = scheduler.evaluate(now + Duration::minutes(30), Some(100.0), 1000);
    assert!(matches!(again, FullChargeAction::Hold { .. }));
}

#[test]
fn evaluating_twice_at_the_same_instant_is_idempotent() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    scheduler.request_manual();
    let first = scheduler.evaluate(now, Some(40.0), 1000);
    let next_after_first = scheduler.next_run();
    let last_after_first = scheduler.last_run();
    let second = scheduler.evaluate(now, Some(40.0), 1000);
    assert_eq!(first, second);
    assert_eq!(scheduler.next_run(), next_after_first);
    assert_eq!(scheduler.last_run(), last_after_first);
}

#[test]
fn the_weekly_run_starts_by_itself_when_due() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    let due = scheduler.next_run().unwrap();
    assert_eq!(due, utc(2026, 3, 29, 1));
    let action = scheduler.evaluate(due, Some(60.0), 1000);
    assert_eq!(charge_limit(action), 1000);
    assert_eq!(scheduler.last_run(), Some(due));
    assert_eq!(scheduler.next_run(), Some(utc(2026, 4, 5, 1)));
}

#[test]
fn the_next_run_keeps_local_wall_clock_across_spring_forward() {
    // 29 March 2026: 02:00 CET jumps to 03:00 CEST, so 03:00 local is 01:00 UTC.
    let scheduler = FullChargeScheduler::new(weekly(), utc(2026, 3, 23, 12));
    assert_eq!(scheduler.next_run(), Some(utc(2026, 3, 29, 1)));
}

#[test]
fn the_next_run_keeps_local_wall_clock_in_winter() {
    let scheduler = FullChargeScheduler::new(weekly(), utc(2026, 3, 8, 12));
    assert_eq!(scheduler.next_run(), Some(utc(2026, 3, 15, 2)));
}

#[test]
fn the_autumn_overlap_takes_the_earlier_instant() {
    // 25 October 2026: 02:30 local happens twice (CEST and CET); the earlier one is 00:30 UTC.
    let mut settings = weekly();
    settings.at = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
    let scheduler = FullChargeScheduler::new(settings, utc(2026, 10, 20, 12));
    assert_eq!(
        scheduler.next_run(),
        Some(utc(2026, 10, 25, 0) + Duration::minutes(30))
    );
}

#[test]
fn the_fall_back_week_moves_the_run_an_hour_later_in_utc() {
    // After the switch 03:00 local is CET, so the same wall clock is 02:00 UTC, not 01:00.
    let scheduler = FullChargeScheduler::new(weekly(), utc(2026, 10, 20, 12));
    assert_eq!(scheduler.next_run(), Some(utc(2026, 10, 25, 2)));
}

#[test]
fn a_wall_clock_inside_the_spring_gap_moves_forward() {
    let mut settings = weekly();
    settings.at = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
    let scheduler = FullChargeScheduler::new(settings, utc(2026, 3, 23, 12));
    assert_eq!(
        scheduler.next_run(),
        Some(utc(2026, 3, 29, 1) + Duration::minutes(30))
    );
}

#[test]
fn a_run_that_cannot_reach_the_target_expires() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    scheduler.request_manual();
    scheduler.evaluate(now, Some(40.0), 1000);
    let stale = now + Duration::hours(25);
    assert_eq!(
        scheduler.evaluate(stale, Some(40.0), 1000),
        FullChargeAction::Release
    );
    assert!(!scheduler.is_running());
}

#[test]
fn a_missing_soc_never_counts_as_reaching_the_target() {
    let now = utc(2026, 3, 23, 12);
    let mut scheduler = FullChargeScheduler::new(weekly(), now);
    scheduler.request_manual();
    let action = scheduler.evaluate(now, None, 1000);
    assert_eq!(
        action,
        FullChargeAction::Charge {
            limit_w: 1000,
            target_soc_pct: 100.0
        }
    );
}
