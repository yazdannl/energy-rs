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
    CapacityBounds, LearningDecision, LearningProfile, LearningReport, recency_weight, relearn,
};
use support::learning::{LABEL_KWH, PACK, SPLIT, at, bounds, charges, cycles};
use support::learning_store::MemoryLearningStore;

const TOL: f64 = 1e-9;

#[test]
fn the_report_carries_a_weight_and_a_basis_per_session() {
    let sessions = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    assert_eq!(report.weights.len(), 3);
    assert_eq!(report.weights[0].rank, 0);
    assert!((report.weights[0].weight - 1.0).abs() < TOL);
    assert!((report.weights[1].weight - recency_weight(1)).abs() < TOL);
    assert!((report.weights[2].weight - recency_weight(2)).abs() < TOL);
    assert!(report.weights.iter().all(|weight| weight.kept));
    assert!(report.weights.iter().all(|weight| !weight.fallback));
    for weight in &report.weights {
        assert!((weight.capacity_kwh - LABEL_KWH).abs() < 1e-9);
    }
    assert!((recency_weight(3) - 0.5).abs() < TOL);
}

#[test]
fn the_summary_names_the_decision_and_the_numbers() {
    let sessions = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    let summary = report.summary();
    assert!(summary.contains("capacity_learned"), "{summary}");
    assert!(summary.contains("efficiency_applied"), "{summary}");
    assert!(summary.contains(PACK), "{summary}");
    assert_eq!(summary, report.to_string());
}

#[test]
fn relearn_persists_and_a_thin_pass_keeps_the_profile() {
    let store = MemoryLearningStore::new();
    let good = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let first = relearn(&store, PACK, &good, &bounds(None), at(24)).unwrap();
    assert!(first.capacity.applied);
    let saved = store.profile(PACK).unwrap();
    assert!(saved.capacity_applied);
    assert!((saved.effective_capacity_kwh().unwrap() - LABEL_KWH).abs() < 1e-9);

    let thin = charges(20.0, 40.0, 1.5, SPLIT, 1);
    let second = relearn(&store, PACK, &thin, &bounds(None), at(48)).unwrap();
    assert_eq!(second.capacity.decision, LearningDecision::KeptPrevious);
    let again = store.profile(PACK).unwrap();
    assert!(again.capacity_applied);
    assert!((again.effective_capacity_kwh().unwrap() - LABEL_KWH).abs() < 1e-9);
    assert!(again.efficiency_applied);

    // A third thin pass still sees a confident profile, so the value cannot erode away.
    let third = relearn(&store, PACK, &thin, &bounds(None), at(72)).unwrap();
    assert_eq!(third.capacity.decision, LearningDecision::KeptPrevious);
    assert!((third.capacity.applied_kwh.unwrap() - LABEL_KWH).abs() < 1e-9);
}

#[test]
fn an_empty_runner_has_no_effective_capacity() {
    let store = MemoryLearningStore::new();
    let report = relearn(&store, PACK, &[], &bounds(None), at(24)).unwrap();
    assert!(report.capacity.applied_kwh.is_none());
    assert_eq!(report.capacity.decision, LearningDecision::NoSessions);
    let profile = store.profile(PACK).unwrap();
    assert!(profile.effective_capacity_kwh().is_none());
    assert!(!profile.capacity_applied);
    assert!(!profile.efficiency_applied);
}

#[test]
fn the_profile_survives_a_json_round_trip() {
    let sessions = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let report = LearningReport::learn(PACK, &sessions, &bounds(None), None, at(24));
    let profile = LearningProfile::from_report(&report);
    let encoded = serde_json::to_string(&profile).unwrap();
    let decoded: LearningProfile = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, profile);
    assert!(decoded.efficiency_estimate().is_confident());
    assert_eq!(decoded.capacity_decision, LearningDecision::Applied);
}

#[test]
fn a_label_less_pack_publishes_nothing() {
    let sessions = cycles(20.0, 50.0, LABEL_KWH, SPLIT, 3);
    let no_label = CapacityBounds::new(0.0, None);
    let report = LearningReport::learn(PACK, &sessions, &no_label, None, at(24));
    assert!(!report.capacity.applied);
    assert!(report.capacity.applied_kwh.is_none());
    assert_eq!(report.capacity.decision, LearningDecision::LowConfidence);
}
