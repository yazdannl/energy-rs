// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{
    AGREEMENT_BAND, DEFAULT_ROUND_TRIP, Direction, LearningProfile, LearningReport, SessionRecord,
};
use support::learning::{
    LABEL_KWH, PACK, SPLIT, at, bounds, charge, cycle, cycles, discharge, session, trip,
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
fn one_dominant_cycle_cannot_set_the_estimate() {
    let mut sessions = Vec::new();
    sessions.extend(cycle(20.0, 40.0, LABEL_KWH, SPLIT, at(0)));
    sessions.extend(cycle(20.0, 40.0, LABEL_KWH, SPLIT, at(4)));
    sessions.extend(cycle(10.0, 70.0, LABEL_KWH, 0.8, at(8)));
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert_eq!(report.efficiency.pairs, 3);
    assert_eq!(report.efficiency.round_trip, DEFAULT_ROUND_TRIP);
    assert!(!report.efficiency.applied);
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(
        diagnostics.blocker.as_deref(),
        Some("too_few_effective_pairs")
    );
    assert!(diagnostics.effective_pairs < 3.0, "{diagnostics:?}");
}

#[test]
fn agreeing_cycles_keep_the_median_against_one_outlier() {
    let mut sessions = Vec::new();
    for day in 0..6 {
        sessions.extend(cycle(20.0, 40.0, LABEL_KWH, SPLIT, at(day * 4)));
    }
    sessions.extend(cycle(10.0, 70.0, LABEL_KWH, 0.8, at(24)));
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(28));
    assert!(report.efficiency.applied);
    assert!((report.efficiency.round_trip - SPLIT * SPLIT).abs() < TOL);
    let diagnostics = &report.efficiency.diagnostics;
    assert!((diagnostics.effective_pairs - 5.4).abs() < TOL);
    assert!(diagnostics.dispersion < TOL);
    assert!((report.efficiency.confidence - 0.9).abs() < TOL);
}

#[test]
fn disagreeing_cycles_do_not_publish_a_measurement() {
    let mut sessions = Vec::new();
    for day in 0..3 {
        sessions.extend(cycle(20.0, 50.0, LABEL_KWH, SPLIT, at(day * 4)));
    }
    for day in 0..3 {
        sessions.extend(cycle(20.0, 50.0, LABEL_KWH, 0.96, at(12 + day * 4)));
    }
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(28));
    assert_eq!(report.efficiency.pairs, 6);
    assert_eq!(report.efficiency.round_trip, DEFAULT_ROUND_TRIP);
    assert!(!report.efficiency.applied);
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.blocker.as_deref(), Some("disagreement"));
    assert!(diagnostics.dispersion > AGREEMENT_BAND, "{diagnostics:?}");
    assert!((diagnostics.confidence - 0.0).abs() < TOL);
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

/// Three charge segments separated by brief pauses, a full-SoC hold, then one evening discharge.
fn interrupted_day(start: i64, split: f64) -> Vec<SessionRecord> {
    let energy = 0.8 * LABEL_KWH * split;
    vec![
        charge(20.0, 40.0, LABEL_KWH, split, at(start)),
        charge(40.0, 80.0, LABEL_KWH, split, at(start + 2)),
        charge(80.0, 100.0, LABEL_KWH, split, at(start + 4)),
        session(Direction::Discharge, 100.0, 20.0, energy, at(start + 12)),
    ]
}

#[test]
fn interrupted_charges_a_hold_and_one_discharge_form_closed_cycles() {
    let mut sessions = Vec::new();
    for start in [0, 24, 48] {
        sessions.extend(interrupted_day(start, SPLIT));
    }
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(72));
    assert_eq!(report.efficiency.pairs, 3);
    assert!(report.efficiency.applied);
    assert!((report.efficiency.round_trip - SPLIT * SPLIT).abs() < TOL);
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.sessions_seen, 12);
    assert_eq!(diagnostics.sessions_valid, 12);
    assert_eq!(diagnostics.candidates, 3);
    assert_eq!(diagnostics.refused, 0);
    assert_eq!(diagnostics.reasons.get("accepted"), Some(&3));
    assert!(diagnostics.attempts.iter().all(|attempt| {
        attempt.accepted
            && attempt.charge_segments == 3
            && attempt
                .round_trip
                .is_some_and(|trip| (trip - 0.81).abs() < TOL)
    }));
}

#[test]
fn charge_segments_alone_are_never_a_cycle() {
    let sessions = vec![
        charge(20.0, 40.0, LABEL_KWH, SPLIT, at(0)),
        charge(40.0, 60.0, LABEL_KWH, SPLIT, at(2)),
        charge(60.0, 80.0, LABEL_KWH, SPLIT, at(4)),
    ];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(8));
    assert_eq!(report.efficiency.pairs, 0);
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.sessions_seen, 3);
    assert_eq!(diagnostics.candidates, 0);
    assert_eq!(diagnostics.attempts.len(), 0);
    assert!(!report.efficiency.applied);
}

#[test]
fn each_discharge_pairs_with_the_charge_block_before_it() {
    let mut sessions = Vec::new();
    sessions.extend(cycle(20.0, 50.0, LABEL_KWH, SPLIT, at(0)));
    sessions.push(charge(30.0, 100.0, LABEL_KWH, SPLIT, at(6)));
    sessions.push(discharge(100.0, 30.0, LABEL_KWH, SPLIT, at(9)));
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert_eq!(report.efficiency.pairs, 2);
    let attempts = &report.efficiency.diagnostics.attempts;
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].soc_from_pct, 20.0);
    assert_eq!(
        attempts[1].soc_from_pct, 30.0,
        "the second charge, not the first"
    );
    assert_eq!(attempts[0].at, at(2));
    assert_eq!(attempts[1].at, at(9));
}

#[test]
fn an_open_but_energy_balanced_span_recovers_the_split() {
    // The discharge stops ten points above the charge's start: the net stored energy must be
    // corrected, even though the AC ratio alone would read high.
    let mut sessions = Vec::new();
    for start in [0, 24, 48] {
        sessions.push(charge(20.0, 90.0, LABEL_KWH, SPLIT, at(start)));
        sessions.push(discharge(90.0, 30.0, LABEL_KWH, SPLIT, at(start + 6)));
    }
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(72));
    assert_eq!(report.efficiency.pairs, 3, "{report}");
    assert!((report.efficiency.round_trip - SPLIT * SPLIT).abs() < TOL);
    assert!(
        report
            .efficiency
            .diagnostics
            .attempts
            .iter()
            .all(|attempt| {
                attempt
                    .round_trip
                    .is_some_and(|trip| (trip - 0.81).abs() < TOL)
            })
    );
}

#[test]
fn refusal_reasons_and_implied_round_trips_are_published() {
    let too_long = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        discharge(50.0, 20.0, LABEL_KWH, SPLIT, at(8 * 24)),
    ];
    let report = LearningReport::learn(PACK, &too_long, &bounds(None), None, at(9 * 24));
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.reasons.get("gap_too_long"), Some(&1));
    assert_eq!(diagnostics.attempts[0].reason, "gap_too_long");
    assert!(diagnostics.attempts[0].round_trip.is_some());

    let no_charge = vec![
        discharge(50.0, 20.0, LABEL_KWH, SPLIT, at(0)),
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(2)),
    ];
    let report = LearningReport::learn(PACK, &no_charge, &bounds(None), None, at(4));
    assert_eq!(
        report
            .efficiency
            .diagnostics
            .reasons
            .get("no_charge_before"),
        Some(&1)
    );

    let drifted = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        discharge(70.0, 50.0, LABEL_KWH, SPLIT, at(2)),
    ];
    let report = LearningReport::learn(PACK, &drifted, &bounds(None), None, at(4));
    assert_eq!(
        report.efficiency.diagnostics.reasons.get("hold_drift"),
        Some(&1)
    );
}

#[test]
fn round_trips_outside_the_physical_band_are_refused() {
    for (in_kwh, out_kwh) in [(5.0, 0.5), (0.5, 5.0)] {
        let sessions = vec![
            session(Direction::Charge, 20.0, 50.0, in_kwh, at(0)),
            session(Direction::Discharge, 50.0, 20.0, out_kwh, at(2)),
        ];
        let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(4));
        assert_eq!(report.efficiency.pairs, 0);
        let diagnostics = &report.efficiency.diagnostics;
        assert_eq!(diagnostics.reasons.get("implausible_round_trip"), Some(&1));
        assert_eq!(diagnostics.attempts[0].reason, "implausible_round_trip");
        assert!(!(0.5..=1.0).contains(&diagnostics.attempts[0].round_trip.unwrap()));
    }
}

#[test]
fn the_attempt_history_keeps_only_the_newest_twelve_events_in_order() {
    let mut sessions = Vec::new();
    for index in 0..15 {
        sessions.push(session(
            Direction::Charge,
            20.0,
            50.0,
            0.64,
            at(index * 9 * 24),
        ));
    }
    sessions.extend(cycle(20.0, 50.0, LABEL_KWH, SPLIT, at(15 * 9 * 24)));
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(16 * 9 * 24));
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.block_splits, 15);
    assert_eq!(diagnostics.attempts.len(), 12);
    assert!(
        diagnostics
            .attempts
            .windows(2)
            .all(|pair| pair[0].at <= pair[1].at)
    );
    assert_eq!(
        diagnostics.attempts.last().expect("newest").reason,
        "accepted"
    );
    assert_eq!(
        diagnostics
            .attempts
            .iter()
            .filter(|attempt| attempt.reason == "segment_gap_too_long")
            .count(),
        11
    );
}

#[test]
fn near_boundary_agreement_is_not_confident_enough_to_apply() {
    let mut sessions = Vec::new();
    for (index, value) in [0.80, 0.819, 0.838].into_iter().enumerate() {
        sessions.extend(trip(value, i64::try_from(index).expect("index") * 4));
    }
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert_eq!(report.efficiency.pairs, 3);
    assert_eq!(report.efficiency.round_trip, DEFAULT_ROUND_TRIP);
    assert!(!report.efficiency.applied);
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.blocker.as_deref(), Some("low_confidence"));
    assert!((diagnostics.dispersion - 0.019).abs() < TOL);
    assert!((diagnostics.confidence - 0.025).abs() < TOL);
}

#[test]
fn the_agreement_band_boundary_does_not_publish_a_zero_confidence_value() {
    let mut sessions = Vec::new();
    for (index, value) in [0.80, 0.82, 0.84].into_iter().enumerate() {
        sessions.extend(trip(value, i64::try_from(index).expect("index") * 4));
    }
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    let diagnostics = &report.efficiency.diagnostics;
    assert!((diagnostics.dispersion - AGREEMENT_BAND).abs() < TOL);
    assert!(diagnostics.confidence < TOL, "{diagnostics:?}");
    assert_eq!(diagnostics.blocker.as_deref(), Some("low_confidence"));
    assert_eq!(report.efficiency.round_trip, DEFAULT_ROUND_TRIP);
    assert!(!report.efficiency.applied);
}

#[test]
fn a_stored_low_confidence_profile_is_not_retained() {
    let measured = LearningReport::learn(
        PACK,
        &cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3),
        &bounds(None),
        None,
        at(24),
    );
    let mut prior = LearningProfile::from_report(&measured);
    prior.efficiency_confidence = 0.1;
    let thin = LearningReport::learn(
        PACK,
        &cycle(20.0, 50.0, LABEL_KWH, SPLIT, at(40)),
        &bounds(None),
        Some(&prior),
        at(48),
    );
    assert!(!thin.efficiency.kept_previous);
    assert_eq!(thin.efficiency.round_trip, DEFAULT_ROUND_TRIP);
}

#[test]
fn continuous_charge_segments_merge_across_a_full_hold() {
    let sessions = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        charge(50.0, 80.0, LABEL_KWH, SPLIT, at(8)),
        discharge(80.0, 20.0, LABEL_KWH, SPLIT, at(10)),
    ];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    assert_eq!(report.efficiency.pairs, 1, "{report}");
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.block_splits, 0);
    let attempt = &diagnostics.attempts[0];
    assert_eq!(attempt.charge_segments, 2);
    assert_eq!(attempt.soc_from_pct, 20.0);
}

#[test]
fn an_unusable_discharge_still_separates_charge_blocks() {
    let mut tiny = session(Direction::Discharge, 50.0, 49.0, 0.01, at(4));
    tiny.sample_count = 5;
    let sessions = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        tiny,
        charge(49.0, 80.0, LABEL_KWH, SPLIT, at(6)),
        discharge(80.0, 20.0, LABEL_KWH, SPLIT, at(10)),
    ];
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(12));
    let diagnostics = &report.efficiency.diagnostics;
    assert_eq!(diagnostics.reasons.get("session_short"), Some(&1));
    assert_eq!(diagnostics.candidates, 2);
    let final_leg = diagnostics
        .attempts
        .iter()
        .find(|attempt| attempt.at == at(10))
        .expect("the final discharge candidate");
    assert_eq!(
        final_leg.charge_segments, 1,
        "the short discharge was not bridged"
    );
    assert_eq!(final_leg.soc_from_pct, 49.0);
}

#[test]
fn continuity_guards_split_merged_blocks_at_every_boundary() {
    let too_long = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        charge(50.0, 80.0, LABEL_KWH, SPLIT, at(8 * 24)),
    ];
    let report = LearningReport::learn(PACK, &too_long, &bounds(None), None, at(9 * 24));
    assert_eq!(
        report
            .efficiency
            .diagnostics
            .reasons
            .get("segment_gap_too_long"),
        Some(&1)
    );

    let drifted = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        charge(60.0, 80.0, LABEL_KWH, SPLIT, at(2)),
    ];
    let report = LearningReport::learn(PACK, &drifted, &bounds(None), None, at(4));
    assert_eq!(
        report
            .efficiency
            .diagnostics
            .reasons
            .get("segment_hold_drift"),
        Some(&1)
    );

    let overlapping = vec![
        charge(20.0, 50.0, LABEL_KWH, SPLIT, at(0)),
        charge(50.0, 80.0, LABEL_KWH, SPLIT, at(0)),
    ];
    let report = LearningReport::learn(PACK, &overlapping, &bounds(None), None, at(4));
    assert_eq!(
        report.efficiency.diagnostics.reasons.get("segment_overlap"),
        Some(&1)
    );
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
    // Six 30-point cycles cover 180 SoC points: the efficiency is certain, the capacity is not.
    assert!(six.capacity.confidence < 0.3, "{six}");
    assert!((six.capacity.soc_points - 180.0).abs() < TOL);
    assert_eq!(six.capacity.cycles_used, 6);
}
