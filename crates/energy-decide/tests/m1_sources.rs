//! M1 scenarios S06, S66 and S126: required completeness, optional zero, one-time notices.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::horizon::{
    EmptyReason, GridSource, HorizonRefusal, NoticeLog, SourceOutcome, sum_grid_sources,
};

#[test]
fn s06_empty_required_source_is_skipped_once_when_redundant_source_is_full() {
    let mut notices = NoticeLog::default();
    let mut coverage = Vec::new();
    let sources = three_sources(Some(vec![None; 96]), None);
    let first = sum_grid_sources(&sources, 96, &mut notices, &mut coverage).unwrap();
    let second = sum_grid_sources(&sources, 96, &mut notices, &mut coverage).unwrap();
    assert_eq!(notices.emitted.len(), 1);
    assert_eq!(notices.emitted[0], "source_missing:p1_history:grid_import");
    assert!(first.values.iter().all(|value| *value == 6.0));
    assert_eq!(first.values, second.values);
    assert_eq!(
        first.partial_sources,
        vec!["p1_history:grid_import".to_owned()]
    );
    let row = first
        .coverage
        .iter()
        .find(|row| row.source_id == "p1_history:grid_import")
        .unwrap();
    assert_eq!(row.outcome, SourceOutcome::PartialRequired);
    assert_eq!(row.empty_reason, Some(EmptyReason::AllEmpty));
}

#[test]
fn s66_optional_counter_is_zero_and_all_empty_category_refuses() {
    let mut notices = NoticeLog::default();
    let mut coverage = Vec::new();
    let sources = three_sources(Some(vec![None; 96]), Some(vec![None; 96]));
    let accepted = sum_grid_sources(&sources, 96, &mut notices, &mut coverage).unwrap();
    let optional = accepted
        .coverage
        .iter()
        .find(|row| row.source_id == "battery_counter")
        .unwrap();
    assert_eq!(optional.outcome, SourceOutcome::OptionalZero);
    assert_eq!(optional.slots_missing, 96);
    assert_eq!(optional.empty_reason, Some(EmptyReason::AllEmpty));

    let no_redundancy = vec![sources[0].clone(), sources[2].clone(), sources[3].clone()];
    let refusal = sum_grid_sources(&no_redundancy, 96, &mut notices, &mut coverage)
        .err()
        .unwrap();
    assert_eq!(
        refusal,
        HorizonRefusal::InputsUnavailable {
            missing_slots: 96,
            source_id: "p1_history:grid_import".to_owned(),
            empty_reason: Some(EmptyReason::AllEmpty),
        }
    );
    assert!(
        coverage
            .iter()
            .any(|row| row.outcome == SourceOutcome::InputsUnavailable)
    );
}

#[test]
fn s126_single_missing_required_slot_refuses_without_zero_fill() {
    let mut missing = vec![Some(1.0); 96];
    missing[7] = None;
    let sources = vec![
        GridSource {
            source_id: "p1_history:grid_import".to_owned(),
            category: "grid_import",
            required: true,
            available: true,
            slots: missing,
        },
        GridSource {
            source_id: "battery_counter".to_owned(),
            category: "battery_charge",
            required: false,
            available: true,
            slots: vec![None; 96],
        },
        GridSource {
            source_id: "fixture:grid_export".to_owned(),
            category: "grid_export",
            required: true,
            available: true,
            slots: vec![Some(2.0); 96],
        },
        GridSource {
            source_id: "fixture:pv".to_owned(),
            category: "pv",
            required: true,
            available: true,
            slots: vec![Some(3.0); 96],
        },
    ];
    let mut notices = NoticeLog::default();
    let mut coverage = Vec::new();
    let refusal = sum_grid_sources(&sources, 96, &mut notices, &mut coverage)
        .err()
        .unwrap();
    assert_eq!(
        refusal,
        HorizonRefusal::InputsUnavailable {
            missing_slots: 1,
            source_id: "p1_history:grid_import".to_owned(),
            empty_reason: Some(EmptyReason::MissingSlots),
        }
    );
    let required = coverage
        .iter()
        .find(|row| row.source_id == "p1_history:grid_import")
        .unwrap();
    assert_eq!(required.slots_missing, 1);
    assert_eq!(required.outcome, SourceOutcome::InputsUnavailable);
    assert!(
        coverage
            .iter()
            .any(|row| row.outcome == SourceOutcome::OptionalZero)
    );
}

#[test]
fn m1b_two_complete_alternatives_are_counted_once() {
    let mut notices = NoticeLog::default();
    let mut coverage = Vec::new();
    let sources = three_sources(None, None);
    let summed = sum_grid_sources(&sources, 96, &mut notices, &mut coverage).unwrap();
    assert!(summed.values.iter().all(|value| *value == 6.0));
    let redundant = summed
        .coverage
        .iter()
        .find(|row| row.source_id == "fixture:grid_import")
        .unwrap();
    assert_eq!(redundant.outcome, SourceOutcome::Redundant);
    assert!(
        summed
            .partial_sources
            .contains(&"fixture:grid_import".to_owned())
    );
    assert!(
        notices
            .emitted
            .contains(&"source_redundant:fixture:grid_import".to_owned())
    );
}

#[test]
fn m1b_unavailable_sources_never_satisfy_or_contribute() {
    let mut notices = NoticeLog::default();
    let mut coverage = Vec::new();
    let mut sources = three_sources(None, Some(vec![Some(999.0); 96]));
    sources[0].available = false;
    sources[4].available = false;
    let summed = sum_grid_sources(&sources, 96, &mut notices, &mut coverage).unwrap();
    assert!(summed.values.iter().all(|value| *value == 6.0));
    let required = summed
        .coverage
        .iter()
        .find(|row| row.source_id == "p1_history:grid_import")
        .unwrap();
    assert_eq!(required.slots_present, 0);
    assert_eq!(required.slots_missing, 96);
    assert_eq!(required.outcome, SourceOutcome::PartialRequired);
    let optional = summed
        .coverage
        .iter()
        .find(|row| row.source_id == "battery_counter")
        .unwrap();
    assert_eq!(optional.outcome, SourceOutcome::OptionalZero);
    assert!(
        notices
            .emitted
            .contains(&"source_unavailable:p1_history:grid_import".to_owned())
    );

    let mut only = vec![sources[0].clone(), sources[2].clone(), sources[3].clone()];
    only[0].slots = vec![Some(1.0); 96];
    let refusal = sum_grid_sources(&only, 96, &mut notices, &mut coverage)
        .err()
        .unwrap();
    assert_eq!(
        refusal,
        HorizonRefusal::InputsUnavailable {
            missing_slots: 96,
            source_id: "p1_history:grid_import".to_owned(),
            empty_reason: Some(EmptyReason::AllEmpty),
        }
    );
}

#[test]
fn m1b_optional_source_with_gaps_contributes_zero() {
    let mut notices = NoticeLog::default();
    let mut coverage = Vec::new();
    let mut partial = vec![Some(100.0); 48];
    partial.extend(vec![None; 48]);
    let sources = three_sources(None, Some(partial));
    let summed = sum_grid_sources(&sources, 96, &mut notices, &mut coverage).unwrap();
    assert!(summed.values.iter().all(|value| *value == 6.0));
    let optional = summed
        .coverage
        .iter()
        .find(|row| row.source_id == "battery_counter")
        .unwrap();
    assert_eq!(optional.outcome, SourceOutcome::OptionalZero);
    assert_eq!(optional.slots_present, 48);
    assert_eq!(optional.empty_reason, Some(EmptyReason::MissingSlots));
}

fn three_sources(
    empty_import: Option<Vec<Option<f64>>>,
    optional: Option<Vec<Option<f64>>>,
) -> Vec<GridSource> {
    let mut sources = vec![
        GridSource {
            source_id: "p1_history:grid_import".to_owned(),
            category: "grid_import",
            required: true,
            available: true,
            slots: empty_import.unwrap_or_else(|| vec![Some(1.0); 96]),
        },
        GridSource {
            source_id: "fixture:grid_import".to_owned(),
            category: "grid_import",
            required: true,
            available: true,
            slots: vec![Some(1.0); 96],
        },
        GridSource {
            source_id: "fixture:grid_export".to_owned(),
            category: "grid_export",
            required: true,
            available: true,
            slots: vec![Some(2.0); 96],
        },
        GridSource {
            source_id: "fixture:pv".to_owned(),
            category: "pv",
            required: true,
            available: true,
            slots: vec![Some(3.0); 96],
        },
    ];
    if let Some(slots) = optional {
        sources.push(GridSource {
            source_id: "battery_counter".to_owned(),
            category: "battery_charge",
            required: false,
            available: true,
            slots,
        });
    }
    sources
}
