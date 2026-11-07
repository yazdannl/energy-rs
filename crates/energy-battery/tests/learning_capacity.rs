// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    // Fixture indices are single-digit hour offsets, never near i64::MAX.
    clippy::cast_possible_wrap,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{
    CapacityBounds, Direction, LearningDecision, LearningProfile, LearningReport,
};
use support::learning::{
    LABEL_KWH, PACK, SPLIT, at, bounds, charge, charges, cycles, discharges, session,
};

/// Synthetic sessions carry no noise, so a recovery must be exact, not close.
const TOL: f64 = 1e-9;

fn capacity_of(report: &LearningReport) -> f64 {
    report.capacity.applied_kwh.unwrap()
}

#[test]
fn recovers_a_known_charge_side_capacity_exactly() {
    let sessions = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert!((capacity_of(&report) - LABEL_KWH).abs() < TOL);
    assert!((report.efficiency.round_trip - SPLIT * SPLIT).abs() < TOL);
    assert_eq!(report.capacity.decision, LearningDecision::Applied);
    assert!(report.capacity.applied);
    assert_eq!(report.used, 3);
}

#[test]
fn falls_back_to_the_discharge_side_without_charge_sessions() {
    let sessions = discharges(60.0, 30.0, LABEL_KWH, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert!((capacity_of(&report) - LABEL_KWH).abs() < TOL);
    assert!(report.weights.iter().all(|weight| weight.fallback));
    assert!(
        report
            .weights
            .iter()
            .all(|weight| weight.direction == Direction::Discharge)
    );
}

#[test]
fn recency_makes_the_newest_sessions_win() {
    let mut sessions = Vec::new();
    for (index, capacity) in [2.3, 2.3, 2.3, 1.5, 1.5, 1.5].into_iter().enumerate() {
        sessions.push(charge(20.0, 40.0, capacity, SPLIT, at(index as i64 * 2)));
    }
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert!((capacity_of(&report) - 1.5).abs() < 1e-9, "{report}");
    let newest = report.weights.first().unwrap();
    let oldest = report.weights.last().unwrap();
    assert_eq!(newest.rank, 0);
    assert!(newest.weight > oldest.weight);
    assert!(oldest.weight < 0.4);
}

#[test]
fn the_newest_sessions_alone_would_explain_the_same_number() {
    let sessions = charges(60.0, 80.0, 1.5, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert!((capacity_of(&report) - 1.5).abs() < 1e-9);
}

#[test]
fn a_median_absolute_deviation_trim_rejects_one_bad_session() {
    let sessions = vec![
        charge(20.0, 50.0, 4.0, SPLIT, at(0)),
        charge(20.0, 50.0, 1.5, SPLIT, at(2)),
        charge(20.0, 50.0, 1.6, SPLIT, at(4)),
        charge(20.0, 50.0, 1.45, SPLIT, at(6)),
        charge(20.0, 50.0, 1.55, SPLIT, at(8)),
    ];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert!((capacity_of(&report) - 1.55).abs() < 1e-9, "{report}");
    assert_eq!(report.used, 4);
    assert_eq!(report.capacity.trimmed, 1);
    // Weights are ordered newest first, so the oldest (the bad one) is last.
    assert!(report.capacity.kept[..4].iter().all(|kept| *kept));
    assert!(!report.capacity.kept[4]);
}

#[test]
fn the_band_stops_an_estimate_above_the_label() {
    let sessions = charges(20.0, 40.0, 3.0, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert!((report.capacity.raw_kwh.unwrap() - 3.0).abs() < 1e-9);
    assert!((capacity_of(&report) - LABEL_KWH * 1.25).abs() < 1e-9);
    assert_eq!(report.capacity.decision, LearningDecision::ClampedHigh);
}

#[test]
fn the_reported_capacity_is_a_hard_ceiling() {
    let sessions = charges(20.0, 40.0, 2.0, SPLIT, 3);
    let reported = CapacityBounds::new(LABEL_KWH, Some(1.8));
    let report = LearningReport::learn(PACK, &sessions, &reported, None, at(12));
    assert!((capacity_of(&report) - 1.8).abs() < 1e-9);
    assert_eq!(report.capacity.decision, LearningDecision::ClampedHigh);
    assert_eq!(report.capacity.band_high_kwh, 1.8);
}

#[test]
fn a_reported_capacity_above_the_band_does_not_widen_it() {
    let sessions = charges(20.0, 40.0, 2.35, SPLIT, 3);
    let reported = CapacityBounds::new(LABEL_KWH, Some(2.5));
    let report = LearningReport::learn(PACK, &sessions, &reported, None, at(12));
    assert!((capacity_of(&report) - 2.35).abs() < 1e-9);
    assert_eq!(report.capacity.decision, LearningDecision::Applied);
    assert!((report.capacity.band_high_kwh - LABEL_KWH * 1.25).abs() < 1e-9);
}

#[test]
fn refuses_incomplete_short_reversed_and_foreign_sessions() {
    let mut short = charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0));
    short.sample_count = 5;
    let mut incomplete = charge(20.0, 50.0, LABEL_KWH, SPLIT, at(2));
    incomplete.complete = false;
    let reversed = session(Direction::Charge, 60.0, 40.0, 0.5, at(4));
    let mut foreign = charge(20.0, 50.0, LABEL_KWH, SPLIT, at(6));
    foreign.pack_id = "OTHERPACK".to_owned();
    let sessions = vec![short, incomplete, reversed, foreign];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert_eq!(report.refused.len(), 4);
    assert_eq!(report.used, 0);
    assert!(report.capacity.applied_kwh.is_none());
    assert_eq!(report.capacity.decision, LearningDecision::NoSessions);
    let slugs: Vec<&str> = report
        .refused
        .iter()
        .map(energy_battery::Refusal::slug)
        .collect();
    assert!(slugs.contains(&"session_short"));
    assert!(slugs.contains(&"session_incomplete"));
    assert!(slugs.contains(&"session_direction_reversal"));
    assert!(slugs.contains(&"session_foreign_pack"));
}

#[test]
fn thin_data_keeps_the_previous_capacity() {
    let good = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let first = LearningReport::learn(PACK, &good, &bounds(None), None, at(24));
    let previous = LearningProfile::from_report(&first);
    let thin = charges(20.0, 40.0, 1.5, SPLIT, 1);
    let second = LearningReport::learn(PACK, &thin, &bounds(None), Some(&previous), at(48));
    assert_eq!(second.capacity.decision, LearningDecision::KeptPrevious);
    assert!(!second.capacity.applied);
    assert!((capacity_of(&second) - LABEL_KWH).abs() < TOL);
    assert_eq!(second.used, 1);
    assert_eq!(second.capacity.confidence, first.capacity.confidence);
}
