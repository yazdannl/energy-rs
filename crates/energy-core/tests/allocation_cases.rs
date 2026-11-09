//! Allocation cases: staggering over the fewest packs, SoC order, ceilings and the deadband.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::allocation::{ALLOCATION_DEADBAND_W, Allocation, SplitInput};
use energy_core::plan::PackState;
use support::state;

#[test]
fn discharge_uses_the_fullest_pack_alone() {
    let packs = vec![state("a", 4.0, 4000.0, 90.0), state("b", 4.0, 4000.0, 30.0)];
    let split = split_of(-1000.0, &packs, 0.0, 1.0, &[], true);
    assert_eq!(split.watts_for("a"), Some(-1000.0));
    assert_eq!(split.watts_for("b"), Some(0.0), "one pack is enough");
    assert_eq!(split.unmet_w, 0.0);
}

#[test]
fn charge_uses_the_emptiest_pack_alone() {
    let packs = vec![state("a", 4.0, 4000.0, 30.0), state("b", 4.0, 4000.0, 80.0)];
    let split = split_of(700.0, &packs, 0.0, 1.0, &[], true);
    assert_eq!(split.watts_for("a"), Some(700.0));
    assert_eq!(split.watts_for("b"), Some(0.0));
}

#[test]
fn a_saturated_pack_spills_into_the_next_in_soc_order() {
    let packs = vec![
        state("a", 4.0, 800.0, 20.0),
        state("b", 4.0, 800.0, 40.0),
        state("c", 4.0, 800.0, 60.0),
    ];
    let split = split_of(1500.0, &packs, 0.0, 1.0, &[], true);
    assert_eq!(split.watts_for("a"), Some(800.0));
    assert_eq!(split.watts_for("b"), Some(700.0));
    assert_eq!(split.watts_for("c"), Some(0.0));
    assert_eq!(split.allocated_w, 1500.0);
}

#[test]
fn a_sliver_never_starts_another_pack() {
    let packs = vec![state("a", 4.0, 800.0, 60.0), state("b", 4.0, 800.0, 40.0)];
    let split = split_of(-830.0, &packs, 0.0, 1.0, &[], true);
    assert_eq!(split.watts_for("a"), Some(-800.0));
    assert_eq!(split.watts_for("b"), Some(0.0), "30 W is not worth a pack");
    // The 30 W gap sits inside the allowed deviation, so nothing is reported unmet.
    assert_eq!(split.unmet_w, 0.0);
}

#[test]
fn a_target_below_the_minimum_runs_nothing() {
    let packs = vec![state("a", 4.0, 800.0, 60.0)];
    let split = split_of(-30.0, &packs, 0.0, 1.0, &[], true);
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(split.allocated_w, 0.0);
}

#[test]
fn a_pack_that_can_only_take_a_sliver_is_skipped() {
    let packs = vec![state("a", 4.0, 20.0, 20.0), state("b", 4.0, 800.0, 40.0)];
    let split = split_of(600.0, &packs, 0.0, 1.0, &[], true);
    assert_eq!(split.watts_for("a"), Some(0.0), "20 W is below the minimum");
    assert_eq!(split.watts_for("b"), Some(600.0));
}

#[test]
fn the_proportional_split_is_still_available() {
    let packs = vec![state("a", 4.0, 4000.0, 90.0), state("b", 4.0, 4000.0, 30.0)];
    let split = split_of(-1000.0, &packs, 0.0, 1.0, &[], false);
    let a = split.watts_for("a").unwrap();
    let b = split.watts_for("b").unwrap();
    assert!(a < 0.0 && b < 0.0, "staggering off spreads over the fleet");
    assert!((a + b + 1000.0).abs() < 1e-6);
}

#[test]
fn a_pack_never_exceeds_its_own_ceiling() {
    let packs = vec![state("a", 4.0, 300.0, 50.0), state("b", 4.0, 3000.0, 50.0)];
    let split = split_of(-5000.0, &packs, 0.0, 0.25, &[], true);
    assert_eq!(split.watts_for("a"), Some(-300.0));
    assert_eq!(split.watts_for("b"), Some(-3000.0));
    assert_eq!(split.unmet_w, -1700.0);
}

#[test]
fn a_full_fleet_cannot_charge() {
    let packs = vec![state("a", 4.0, 1000.0, 95.0), state("b", 4.0, 1000.0, 95.0)];
    let split = split_of(1000.0, &packs, 0.0, 0.25, &[], true);
    assert_eq!(split.allocated_w, 0.0);
    assert_eq!(split.unmet_w, 1000.0);
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(split.watts_for("b"), Some(0.0));
}

#[test]
fn a_blocked_pack_gets_no_discharge_and_the_other_takes_it() {
    let packs = vec![state("a", 4.0, 2000.0, 80.0), state("b", 4.0, 2000.0, 40.0)];
    let split = split_of(-500.0, &packs, 0.0, 0.25, &["a"], true);
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(split.watts_for("b"), Some(-500.0));
}

#[test]
fn a_target_inside_the_deadband_is_not_allocated() {
    let packs = vec![state("a", 4.0, 1000.0, 50.0)];
    let split = split_of(ALLOCATION_DEADBAND_W / 2.0, &packs, 0.0, 0.25, &[], true);
    assert_eq!(split.allocated_w, 0.0);
    assert_eq!(split.unmet_w, 0.0);
    assert_eq!(split.watts_for("a"), Some(0.0));
}

#[test]
fn the_split_is_deterministic_in_id_order() {
    let packs = vec![state("b", 4.0, 1000.0, 50.0), state("a", 4.0, 1000.0, 50.0)];
    let first = split_of(500.0, &packs, 0.0, 0.25, &[], true);
    let second = split_of(500.0, &packs, 0.0, 0.25, &[], true);
    assert_eq!(first, second);
    let ids: Vec<&str> = first.packs.iter().map(|p| p.pack_id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
}

/// The fleet split with the policy these cases assume.
fn split_of(
    target: f64,
    packs: &[PackState],
    reserve: f64,
    hours: f64,
    blocked: &[&str],
    staggering: bool,
) -> Allocation {
    let input = SplitInput {
        reserve_soc_pct: reserve,
        slot_hours: hours,
        block_discharge: blocked,
        min_active_w: 50.0,
        staggering,
    };
    Allocation::split(target, packs, &input)
}
