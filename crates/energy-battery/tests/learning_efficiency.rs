// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{DEFAULT_ROUND_TRIP, Direction, LearningProfile, LearningReport};
use support::learning::{
    LABEL_KWH, PACK, SPLIT, at, bounds, charge, cycle, cycles, discharge, session,
};

/// Synthetic cycles carry no noise, so a recovery must be exact, not close.
const TOL: f64 = 1e-9;

#[test]
fn recovers_a_known_round_trip_exactly() {
    let sessions = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert!((report.efficiency.round_trip - SPLIT * SPLIT).abs() < TOL);
    assert!((report.efficiency.eta_charge - SPLIT).abs() < TOL);
    assert!((report.efficiency.eta_discharge - SPLIT).abs() < TOL);
    assert_eq!(report.efficiency.pairs, 3);
    assert!(report.efficiency.applied);
    assert!(!report.efficiency.kept_previous);
}

#[test]
fn a_deeper_cycle_dominates_the_weighted_median() {
    let mut sessions = Vec::new();
    sessions.extend(cycle(20.0, 40.0, LABEL_KWH, SPLIT, at(0)));
    sessions.extend(cycle(20.0, 40.0, LABEL_KWH, SPLIT, at(4)));
    sessions.extend(cycle(10.0, 70.0, LABEL_KWH, 0.8, at(8)));
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert!((report.efficiency.round_trip - 0.64).abs() < TOL);
}

#[test]
fn a_cycle_cannot_cross_two_packs() {
    let mut other = discharge(50.0, 20.0, LABEL_KWH, SPLIT, at(2));
    other.pack_id = "OTHERPACK".to_owned();
    let sessions = vec![charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)), other];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(4));
    assert_eq!(report.efficiency.pairs, 0);
    assert_eq!(report.used, 1);
}

#[test]
fn thin_data_keeps_the_previous_round_trip() {
    let good = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let first = LearningReport::learn(PACK, &good, &bounds(None), None, at(24));
    let previous = LearningProfile::from_report(&first);
    assert!(previous.efficiency_applied);
    let thin = cycle(20.0, 50.0, LABEL_KWH, SPLIT, at(40));
    let second = LearningReport::learn(PACK, &thin, &bounds(None), Some(&previous), at(48));
    assert!(second.efficiency.kept_previous);
    assert!(!second.efficiency.applied);
    assert!((second.efficiency.round_trip - SPLIT * SPLIT).abs() < TOL);
    assert_eq!(second.efficiency.pairs, 1);
}

#[test]
fn no_cycle_at_all_leaves_the_typed_default() {
    let sessions = vec![charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0))];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert_eq!(report.efficiency.round_trip, DEFAULT_ROUND_TRIP);
    assert!(!report.efficiency.applied);
    assert!(!report.efficiency.kept_previous);
    assert_eq!(report.efficiency.pairs, 0);
}

#[test]
fn a_discharge_that_stops_short_is_not_a_cycle() {
    let sessions = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        discharge(50.0, 40.0, LABEL_KWH, SPLIT, at(2)),
    ];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(4));
    assert_eq!(report.efficiency.pairs, 0);
    assert_eq!(report.efficiency.pairs_refused, 1);
    assert!(!report.efficiency.applied);
}

#[test]
fn a_cycle_that_drifted_between_its_legs_is_refused() {
    let drifted = session(Direction::Discharge, 60.0, 20.0, 0.3, at(2));
    let sessions = vec![charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)), drifted];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(4));
    assert_eq!(report.efficiency.pairs, 0);
    assert_eq!(report.efficiency.pairs_refused, 1);
}

#[test]
fn confidence_reaches_one_at_six_cycles() {
    let three = LearningReport::learn(
        PACK,
        &cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3),
        &bounds(None),
        None,
        at(24),
    );
    assert!((three.efficiency.confidence - 0.5).abs() < TOL);
    let six = LearningReport::learn(
        PACK,
        &cycles(20.0, 50.0, LABEL_KWH, SPLIT, 6),
        &bounds(None),
        None,
        at(48),
    );
    assert!((six.efficiency.confidence - 1.0).abs() < TOL);
    assert!((six.capacity.confidence - 1.0).abs() < TOL);
}
