//! M1 scenarios S01-S03, S61-S62 and S125: coverage, gaps and precedence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::horizon::{HorizonOutcome, HorizonRefusal, PriceSlot, assemble_horizon};
use support::{input, price, prices, ts};

#[test]
fn s01_missing_interior_slot_is_price_gap() {
    let mut slots = prices("2026-11-22T15:45:00Z", 120, 0.4);
    slots.remove(17);
    let attempt = assemble_horizon(&input("2026-11-22T15:47:00Z", slots));
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::PriceGap {
            first_missing_t_start: ts("2026-11-22T20:00:00Z"),
            missing_count: 1,
        })
    );
    assert!(attempt.horizon().is_none());
}

#[test]
fn s02_hourly_first_slot_is_price_resolution() {
    let mut slots = prices("2026-11-22T15:45:00Z", 120, 0.4);
    slots[0].slot_seconds = 3600;
    let attempt = assemble_horizon(&input("2026-11-22T15:47:00Z", slots));
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::PriceResolution {
            expected_s: 900,
            got_s: 3600,
            t_start: ts("2026-11-22T15:45:00Z"),
        })
    );
    assert!(attempt.horizon().is_none());
}

#[test]
fn s03_book_grows_from_20_to_84_to_120_slots() {
    let start = ts("2026-11-22T15:45:00Z");
    let first = indexed(start, 0, 20);
    let second = indexed(start, 0, 84);
    let third = indexed(start, 0, 120);
    let first = assemble_horizon(&input("2026-11-22T15:47:00Z", first));
    let second = assemble_horizon(&input("2026-11-22T15:47:00Z", second));
    let third = assemble_horizon(&input("2026-11-22T15:47:00Z", third));
    assert_eq!(
        first.refusal(),
        Some(&HorizonRefusal::InsufficientCoverage {
            actual: 20,
            requested: 120,
            minimum: 96,
        })
    );
    assert_eq!(
        second.refusal(),
        Some(&HorizonRefusal::InsufficientCoverage {
            actual: 84,
            requested: 120,
            minimum: 96,
        })
    );
    assert_eq!(third.horizon().map_or(0, |h| h.slots.len()), 120);
    assert_eq!(20 + 64 + 36, 120);
}

#[test]
fn s61_seventeen_slots_refuse_without_degraded_acceptance() {
    let slots: Vec<_> = (0..17)
        .map(|index| price_from_index(ts("2026-11-22T15:45:00Z"), index, 0.4))
        .collect();
    let attempt = assemble_horizon(&input("2026-11-22T15:47:00Z", slots));
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::InsufficientCoverage {
            actual: 17,
            requested: 120,
            minimum: 96,
        })
    );
    assert_eq!(
        attempt.coverage.first_missing_t_start,
        Some(ts("2026-11-22T20:00:00Z"))
    );
    assert!(attempt.horizon().is_none());
}

#[test]
fn s62_resolution_precedes_price_gap() {
    let mut slots = prices("2026-11-22T15:45:00Z", 120, 0.4);
    slots[0].slot_seconds = 3600;
    slots.remove(1);
    let attempt = assemble_horizon(&input("2026-11-22T15:47:00Z", slots));
    assert!(matches!(
        attempt.refusal(),
        Some(HorizonRefusal::PriceResolution { .. })
    ));
    assert!(!matches!(
        attempt.refusal(),
        Some(HorizonRefusal::PriceGap { .. })
    ));
}

#[test]
fn s125_missing_tail_is_provisional_then_degraded() {
    let attempt = assemble_horizon(&input(
        "2026-11-22T15:47:00Z",
        indexed(ts("2026-11-22T15:45:00Z"), 0, 108),
    ));
    assert_eq!(attempt.provisional_outcome, HorizonOutcome::TailAccepted);
    assert_eq!(attempt.coverage.missing_slots, 12);
    assert_eq!(
        attempt.coverage.first_missing_t_start,
        Some(ts("2026-11-23T18:45:00Z"))
    );
    let horizon = attempt.horizon().unwrap();
    assert_eq!(horizon.outcome, HorizonOutcome::DegradedAccepted);
    assert_eq!(horizon.actual_slots(), 108);
}

#[test]
fn s125_interior_gap_is_fatal() {
    let mut slots = indexed(ts("2026-11-22T15:45:00Z"), 0, 108);
    slots.remove(40);
    let attempt = assemble_horizon(&input("2026-11-22T15:47:00Z", slots));
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::PriceGap {
            first_missing_t_start: ts("2026-11-23T01:45:00Z"),
            missing_count: 1,
        })
    );
    assert_eq!(attempt.provisional_outcome, HorizonOutcome::Healthy);
}

#[test]
fn s125_request_and_resolution_precede_gap_and_coverage() {
    let mut request = input(
        "2026-11-22T15:47:00Z",
        indexed(ts("2026-11-22T15:45:00Z"), 0, 17),
    );
    request.requested_slots = 121;
    request.slots[0].slot_seconds = 3600;
    let refusal = assemble_horizon(&request).refusal().cloned();
    assert!(matches!(
        refusal,
        Some(HorizonRefusal::HorizonSlots { slots: 121 })
    ));

    let mut resolution = input(
        "2026-11-22T15:47:00Z",
        indexed(ts("2026-11-22T15:45:00Z"), 0, 17),
    );
    resolution.slots[0].slot_seconds = 3600;
    let refusal = assemble_horizon(&resolution).refusal().cloned();
    assert!(matches!(
        refusal,
        Some(HorizonRefusal::PriceResolution { .. })
    ));
}

#[test]
fn s125_gap_coverage_stale_and_forecast_precedence() {
    let start = ts("2026-11-22T15:45:00Z");
    let mut gap = input("2026-11-22T15:47:00Z", indexed(start, 0, 108));
    gap.slots.remove(40);
    let mut short = input("2026-11-22T15:47:00Z", indexed(start, 0, 17));
    short.published_at = start - 30_000;
    let mut forecast = input("2026-11-22T15:47:00Z", indexed(start, 0, 108));
    forecast.forecast_available = false;
    assert!(matches!(
        assemble_horizon(&gap).refusal(),
        Some(HorizonRefusal::PriceGap { .. })
    ));
    assert!(matches!(
        assemble_horizon(&short).refusal(),
        Some(HorizonRefusal::InsufficientCoverage { .. })
    ));
    assert!(matches!(
        assemble_horizon(&forecast).refusal(),
        Some(HorizonRefusal::ForecastUnavailable { .. })
    ));
}

#[test]
fn s125_tail_with_stale_prices_never_becomes_degraded() {
    let start = ts("2026-11-22T15:45:00Z");
    let mut stale = input("2026-11-22T15:47:00Z", indexed(start, 0, 108));
    stale.published_at = start - 30_000;
    let attempt = assemble_horizon(&stale);
    assert_eq!(attempt.provisional_outcome, HorizonOutcome::TailAccepted);
    assert!(matches!(
        attempt.refusal(),
        Some(HorizonRefusal::StalePrices { .. })
    ));
    assert!(attempt.horizon().is_none());
}

#[test]
fn verified_price_fixture_keeps_today_and_tomorrow_distinct() {
    let today_peak = price("2026-11-22T18:00:00Z", 0.4188);
    let tomorrow = [
        price("2026-11-23T15:45:00Z", 0.5099),
        price("2026-11-23T16:45:00Z", 0.5303),
        price("2026-11-23T17:45:00Z", 0.5059),
    ];
    assert_eq!(today_peak.t_start, ts("2026-11-22T18:00:00Z"));
    assert_eq!(today_peak.buy_all_in, 0.4188);
    assert_eq!(tomorrow[0].buy_all_in, 0.5099);
    assert_eq!(tomorrow[1].buy_all_in, 0.5303);
    assert_eq!(tomorrow[2].buy_all_in, 0.5059);
    assert!(
        tomorrow
            .iter()
            .all(|slot| slot.t_start > today_peak.t_start)
    );
    assert!(
        tomorrow
            .iter()
            .all(|slot| slot.buy_all_in > today_peak.buy_all_in)
    );
}

// Frozen precedence (SPEC M1): request, resolution, interior gap, coverage, stale, inputs,
// forecast; then healthy or degraded tail. S124: 22:45Z netted, 23:00Z not. S126: one missing
// required-source slot refuses with InputsUnavailable{missing_slots:1}.

#[test]
fn m1a_price_gap_precedes_insufficient_coverage() {
    // SPEC.md M1 numbered error precedence: (3) interior PriceGap precedes (4) InsufficientCoverage
    // when actual < 96, so both failures competing must still report the interior gap.
    let start = ts("2026-11-22T15:45:00Z");
    let mut slots = indexed(start, 0, 95);
    slots.remove(17);
    let request = input("2026-11-22T15:47:00Z", slots);
    assert_eq!(
        assemble_horizon(&request).refusal(),
        Some(&HorizonRefusal::PriceGap {
            first_missing_t_start: start + 17 * 900,
            missing_count: 1,
        })
    );
}

#[test]
fn m1a_notice_survives_a_required_source_refusal() {
    let start = ts("2026-11-22T15:45:00Z");
    let mut request = input("2026-11-22T15:47:00Z", indexed(start, 0, 96));
    request.grid_sources[0].slots = vec![None; 96];
    rename_import_source(&mut request.grid_sources);
    let attempt = assemble_horizon(&request);
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::InputsUnavailable {
            missing_slots: 96,
            source_id: "p1_history:grid_import".to_owned(),
            empty_reason: Some(energy_decide::horizon::EmptyReason::AllEmpty),
        })
    );
    assert!(
        attempt
            .notices
            .contains(&"source_missing:p1_history:grid_import".to_owned())
    );
    assert_eq!(attempt.grid_source_coverage[0].slots_missing, 96);
}

#[test]
fn m1a_95_slots_refuse_and_96_slots_degrade_with_exact_coverage() {
    let start = ts("2026-11-22T15:45:00Z");
    let short = assemble_horizon(&input("2026-11-22T15:47:00Z", indexed(start, 0, 95)));
    assert_eq!(
        short.refusal(),
        Some(&HorizonRefusal::InsufficientCoverage {
            actual: 95,
            requested: 120,
            minimum: 96,
        })
    );
    let accepted = assemble_horizon(&input("2026-11-22T15:47:00Z", indexed(start, 0, 96)));
    assert_eq!(accepted.provisional_outcome, HorizonOutcome::TailAccepted);
    assert_eq!(accepted.coverage.actual_slots, 96);
    assert_eq!(accepted.coverage.missing_slots, 24);
    assert_eq!(accepted.coverage.coverage_end, Some(start + 96 * 900));
    assert_eq!(
        accepted.coverage.first_missing_t_start,
        Some(start + 96 * 900)
    );
    let horizon = accepted.horizon().unwrap();
    assert_eq!(horizon.outcome, HorizonOutcome::DegradedAccepted);
    assert_eq!(horizon.actual_slots(), 96);
}

#[test]
fn m1a_request_resolution_and_coverage_precedence_has_exact_values() {
    let start = ts("2026-11-22T15:45:00Z");
    let mut request = input("2026-11-22T15:47:00Z", indexed(start, 0, 17));
    request.requested_slots = 121;
    request.slots[0].slot_seconds = 3600;
    request.published_at = start - 30_000;
    request.forecast_available = false;
    assert_eq!(
        assemble_horizon(&request).refusal(),
        Some(&HorizonRefusal::HorizonSlots { slots: 121 })
    );
    request.requested_slots = 120;
    assert_eq!(
        assemble_horizon(&request).refusal(),
        Some(&HorizonRefusal::PriceResolution {
            expected_s: 900,
            got_s: 3600,
            t_start: start,
        })
    );
    request.slots[0].slot_seconds = 900;
    let attempt = assemble_horizon(&request);
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::InsufficientCoverage {
            actual: 17,
            requested: 120,
            minimum: 96,
        })
    );
    assert_eq!(
        attempt.coverage.first_missing_t_start,
        Some(start + 17 * 900)
    );
    assert_eq!(attempt.coverage.actual_slots, 17);
}

#[test]
fn m1a_stale_then_inputs_then_forecast_precedence_has_exact_evidence() {
    let start = ts("2026-11-22T15:45:00Z");
    let mut tail = input("2026-11-22T15:47:00Z", indexed(start, 0, 108));
    tail.published_at = start - 30_000;
    tail.grid_sources[0].slots = vec![None; 108];
    rename_import_source(&mut tail.grid_sources);
    tail.forecast_available = false;
    assert_eq!(
        assemble_horizon(&tail).refusal(),
        Some(&HorizonRefusal::StalePrices {
            age_s: 30_120,
            limit_s: 21_600,
        })
    );
    tail.published_at = start - 60;
    assert_eq!(
        assemble_horizon(&tail).refusal(),
        Some(&HorizonRefusal::InputsUnavailable {
            missing_slots: 108,
            source_id: "p1_history:grid_import".to_owned(),
            empty_reason: Some(energy_decide::horizon::EmptyReason::AllEmpty),
        })
    );
    tail.grid_sources = support::full_sources(108);
    assert_eq!(
        assemble_horizon(&tail).refusal(),
        Some(&HorizonRefusal::ForecastUnavailable {
            reason: "no_fresh_forecast",
        })
    );
}

#[test]
fn s124_schedule_keeps_the_exclusive_boundary_exact() {
    use energy_decide::horizon::{BudgetState, NetMeteringRegime, settle_slot};

    let last = price("2026-12-31T22:45:00Z", 0.412);
    let first = price("2026-12-31T23:00:00Z", 0.412);
    let regime = NetMeteringRegime::DateDriven;
    let budget = BudgetState::Active {
        remaining_kwh: 10.0,
    };
    let before = settle_slot(&last, &regime, &budget, last.t_start).unwrap();
    let after = settle_slot(&first, &regime, &budget, first.t_start).unwrap();
    assert_eq!((before.netting, before.export_eur_per_kwh), (true, 0.412));
    assert_eq!((after.netting, after.export_eur_per_kwh), (false, 0.18));
}

#[test]
fn s126_one_missing_required_source_slot_reaches_the_attempt() {
    let start = ts("2026-11-22T15:45:00Z");
    let mut request = input("2026-11-22T15:47:00Z", indexed(start, 0, 96));
    let mut missing = vec![Some(1.0); 96];
    missing[7] = None;
    request.grid_sources[0].slots = missing;
    rename_import_source(&mut request.grid_sources);
    let attempt = assemble_horizon(&request);
    assert_eq!(
        attempt.refusal(),
        Some(&HorizonRefusal::InputsUnavailable {
            missing_slots: 1,
            source_id: "p1_history:grid_import".to_owned(),
            empty_reason: Some(energy_decide::horizon::EmptyReason::MissingSlots),
        })
    );
    assert_eq!(attempt.grid_source_coverage[0].slots_present, 95);
}

fn rename_import_source(sources: &mut [energy_decide::horizon::GridSource]) {
    for source in sources.iter_mut() {
        if source.category == "grid_import" {
            "p1_history:grid_import".clone_into(&mut source.source_id);
        }
    }
}

fn indexed(start: i64, from: usize, to: usize) -> Vec<PriceSlot> {
    (from..to)
        .map(|index| price_from_index(start, index, 0.4))
        .collect()
}

fn price_from_index(start: i64, index: usize, buy: f64) -> PriceSlot {
    let mut slot = price("2026-11-22T15:45:00Z", buy);
    slot.t_start = start + i64::try_from(index).unwrap_or(i64::MAX) * 900;
    slot
}
