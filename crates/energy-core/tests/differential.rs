//! The periodic differential: the C core and the brute-force oracle must agree on a real plan.
//!
//! `PLAN.md` §6 makes this the engine's last line of defence: one solve per interval is re-derived
//! by [`energy_opt::oracle`], which shares no arithmetic with the core. These tests prove the check
//! is reachable from the plan builder, that it accepts an honest solve and that it is rate-limited.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::plan::{Differential, HORIZON_SLOTS, PlanInput};
use support::{NOW, flat_book, forecasts, settlement, state};

fn input() -> PlanInput {
    PlanInput::new(
        NOW,
        vec![state("a", 10.0, 2000.0, 50.0)],
        flat_book(NOW, HORIZON_SLOTS),
        settlement(),
    )
    .with_forecasts(
        forecasts(0.0, HORIZON_SLOTS),
        forecasts(0.5, HORIZON_SLOTS),
        NOW,
    )
}

#[test]
fn the_core_and_the_oracle_agree_on_a_real_plan() {
    let check = Differential::new(0);
    assert!(check.due(NOW), "a zero interval checks every solve");
    let plan = input()
        .plan_checked(Some(&check))
        .expect("the core and the oracle must agree");
    assert_eq!(plan.slots.len(), HORIZON_SLOTS);
    assert!(plan.feasible);
}

#[test]
fn a_plan_without_the_check_is_unchanged() {
    let checked = input().plan_checked(Some(&Differential::new(0))).unwrap();
    let plain = input().plan().unwrap();
    assert_eq!(checked, plain);
}

#[test]
fn the_check_is_claimed_once_per_interval() {
    let check = Differential::new(3600);
    assert!(check.due(NOW));
    assert!(!check.due(NOW + 900), "a second solve must skip it");
    assert!(check.due(NOW + 3600), "the next hour runs it again");
}
