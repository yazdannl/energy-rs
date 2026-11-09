//! The latched pack selection: the anti-thrash rule from `docs/PACK_SELECTION.md` §5.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::allocation::{Allocation, Hold, SplitInput};
use energy_core::plan::PackState;
use support::state;

const NOW: i64 = 1_800_000_000;
const HOLD_S: i64 = 1800;

fn input<'a>(direction: &'a str, holder: &'a str, since: i64) -> SplitInput<'a> {
    SplitInput {
        hold: Some(Hold {
            holder,
            direction,
            since,
        }),
        hold_seconds: HOLD_S,
        now: NOW,
        ..SplitInput::default()
    }
}

fn split_of(target: f64, packs: &[PackState], input: &SplitInput<'_>) -> Allocation {
    Allocation::split(target, packs, input)
}

#[test]
fn a_rising_soc_does_not_rotate_the_pack_within_the_hold() {
    // `a` was picked at 20 %; by now it is at 45 % and `b` at 40 %, so SoC order now prefers `b`.
    let packs = vec![state("a", 4.0, 800.0, 45.0), state("b", 4.0, 800.0, 40.0)];
    let held = input("charge", "a", NOW - 600);
    let split = split_of(700.0, &packs, &held);
    assert_eq!(split.watts_for("a"), Some(700.0), "the hold keeps the slot");
    assert_eq!(split.watts_for("b"), Some(0.0));

    let fresh = SplitInput::default();
    let split = split_of(700.0, &packs, &fresh);
    assert_eq!(
        split.watts_for("b"),
        Some(700.0),
        "without the hold the emptiest pack takes it"
    );
}

#[test]
fn the_hold_expires_and_the_better_pack_takes_over() {
    let packs = vec![state("a", 4.0, 800.0, 45.0), state("b", 4.0, 800.0, 40.0)];
    let expired = input("charge", "a", NOW - HOLD_S - 1);
    let split = split_of(700.0, &packs, &expired);
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(split.watts_for("b"), Some(700.0));
}

#[test]
fn a_sign_flip_releases_the_hold() {
    // The hold was taken while charging; this tick discharges, so it must not apply.
    let packs = vec![state("a", 4.0, 800.0, 80.0), state("b", 4.0, 800.0, 20.0)];
    let held = input("charge", "a", NOW);
    let split = split_of(-700.0, &packs, &held);
    assert_eq!(
        split.watts_for("a"),
        Some(-700.0),
        "a is the fullest anyway"
    );
    let held = input("discharge", "a", NOW);
    let split = split_of(-700.0, &packs, &held);
    assert_eq!(
        split.watts_for("a"),
        Some(-700.0),
        "the discharge hold applies"
    );

    // Now the other way round: a charge hold that would pick the wrong pack for a discharge.
    let packs = vec![state("a", 4.0, 800.0, 20.0), state("b", 4.0, 800.0, 80.0)];
    let held = input("discharge", "a", NOW);
    let split = split_of(700.0, &packs, &held);
    assert_eq!(
        split.watts_for("b"),
        Some(0.0),
        "the emptiest pack charges, the stale hold does not apply"
    );
    assert_eq!(split.watts_for("a"), Some(700.0));
}

#[test]
fn a_pack_that_can_no_longer_serve_is_replaced_immediately() {
    // `a` holds the charge slot but sits at its ceiling: no headroom at all.
    let packs = vec![state("a", 4.0, 800.0, 95.0), state("b", 4.0, 800.0, 40.0)];
    let held = input("charge", "a", NOW);
    let split = split_of(700.0, &packs, &held);
    assert_eq!(
        split.watts_for("a"),
        Some(0.0),
        "a hard limit beats the hold"
    );
    assert_eq!(split.watts_for("b"), Some(700.0));
}

#[test]
fn a_hold_on_a_pack_that_is_not_there_is_ignored() {
    let packs = vec![state("b", 4.0, 800.0, 40.0)];
    let held = input("charge", "gone", NOW);
    let split = split_of(700.0, &packs, &held);
    assert_eq!(split.watts_for("b"), Some(700.0));
}

#[test]
fn a_soc_difference_inside_the_band_is_a_tie() {
    // 40 % and 41 %: inside the 2-point band, so it is a tie and the lower id leads.
    let packs = vec![state("b", 4.0, 800.0, 40.0), state("a", 4.0, 800.0, 41.0)];
    let split = split_of(700.0, &packs, &SplitInput::default());
    assert_eq!(
        split.watts_for("a"),
        Some(700.0),
        "a tie, so the id decides"
    );
    assert_eq!(split.watts_for("b"), Some(0.0));

    let packs = vec![state("b", 4.0, 800.0, 36.0), state("a", 4.0, 800.0, 40.0)];
    let split = split_of(700.0, &packs, &SplitInput::default());
    assert_eq!(split.watts_for("a"), Some(0.0));
    assert_eq!(
        split.watts_for("b"),
        Some(700.0),
        "36 and 40 are two bands apart, the SoC decides"
    );
}

#[test]
fn a_zero_hold_bound_disables_the_hold() {
    let packs = vec![state("a", 4.0, 800.0, 45.0), state("b", 4.0, 800.0, 40.0)];
    let mut held = input("charge", "a", NOW);
    held.hold_seconds = 0;
    let split = split_of(700.0, &packs, &held);
    assert_eq!(split.watts_for("b"), Some(700.0));
}
