//! Allocation cases: uneven SoC, a capped pack, a full fleet and the deadband.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::allocation::{ALLOCATION_DEADBAND_W, Allocation};
use support::state;

#[test]
fn discharge_is_balanced_by_remaining_headroom_not_equally() {
    let packs = vec![state("a", 4.0, 4000.0, 90.0), state("b", 4.0, 4000.0, 30.0)];
    let split = Allocation::split(-1000.0, &packs, 0.0, 1.0, &[]);
    let a = split.watts_for("a").unwrap();
    let b = split.watts_for("b").unwrap();
    assert!(a < 0.0 && b < 0.0);
    assert!(a.abs() > b.abs() * 3.0, "a={a} b={b}");
    assert!((a + b + 1000.0).abs() < 1e-6);
    assert_eq!(split.unmet_w, 0.0);
}

#[test]
fn a_pack_never_exceeds_its_own_ceiling() {
    let packs = vec![state("a", 4.0, 300.0, 50.0), state("b", 4.0, 3000.0, 50.0)];
    let split = Allocation::split(-5000.0, &packs, 0.0, 0.25, &[]);
    assert_eq!(split.watts_for("a"), Some(-300.0));
    assert_eq!(split.watts_for("b"), Some(-3000.0));
    assert_eq!(split.unmet_w, -1700.0);
}

#[test]
fn a_full_fleet_cannot_charge() {
    let packs = vec![state("a", 4.0, 1000.0, 95.0), state("b", 4.0, 1000.0, 95.0)];
    let split = Allocation::split(1000.0, &packs, 0.0, 0.25, &[]);
    assert_eq!(split.allocated_w, 0.0);
    assert_eq!(split.unmet_w, 1000.0);
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(split.watts_for("b"), Some(0.0));
}

#[test]
fn a_blocked_pack_gets_no_discharge_and_the_other_takes_it() {
    let packs = vec![state("a", 4.0, 2000.0, 80.0), state("b", 4.0, 2000.0, 40.0)];
    let split = Allocation::split(-500.0, &packs, 0.0, 0.25, &["a"]);
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(split.watts_for("b"), Some(-500.0));
}

#[test]
fn a_target_inside_the_deadband_is_not_allocated() {
    let packs = vec![state("a", 4.0, 1000.0, 50.0)];
    let split = Allocation::split(ALLOCATION_DEADBAND_W / 2.0, &packs, 0.0, 0.25, &[]);
    assert_eq!(split.allocated_w, 0.0);
    assert_eq!(split.unmet_w, 0.0);
    assert_eq!(split.watts_for("a"), Some(0.0));
}

#[test]
fn the_split_is_deterministic_in_id_order() {
    let packs = vec![state("b", 4.0, 1000.0, 50.0), state("a", 4.0, 1000.0, 50.0)];
    let first = Allocation::split(500.0, &packs, 0.0, 0.25, &[]);
    let second = Allocation::split(500.0, &packs, 0.0, 0.25, &[]);
    assert_eq!(first, second);
    let ids: Vec<&str> = first.packs.iter().map(|p| p.pack_id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
}
