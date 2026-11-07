//! Plan builder refusals and determinism.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::plan::{HORIZON_SLOTS, PlanError, PlanInput};
use support::{NOW, book, flat_book, forecasts, settlement, state};

fn input(book: energy_price::PriceBook) -> PlanInput {
    PlanInput::new(
        NOW,
        vec![state("a", 10.0, 2000.0, 50.0)],
        book,
        settlement(),
    )
    .with_forecasts(
        forecasts(0.0, HORIZON_SLOTS),
        forecasts(0.5, HORIZON_SLOTS),
        NOW,
    )
}

#[test]
fn a_price_gap_at_the_end_of_the_horizon_is_refused() {
    let request = input(flat_book(NOW, HORIZON_SLOTS - 1));
    match request.plan() {
        Err(PlanError::PriceGap { t_start }) => {
            assert_eq!(
                t_start,
                NOW + (i64::try_from(HORIZON_SLOTS).unwrap() - 1) * 900
            );
        }
        other => panic!("expected a price gap, got {other:?}"),
    }
}

#[test]
fn an_hourly_book_is_refused_rather_than_resampled() {
    let slots: Vec<energy_price::Slot> = (0..HORIZON_SLOTS / 4)
        .map(|index| {
            energy_price::Slot::new(
                NOW + i64::try_from(index).unwrap() * 3600,
                0.20,
                0.08,
                energy_price::SlotSource::ZonneplanHourly,
            )
        })
        .collect();
    let book = energy_price::PriceBook::new(slots).unwrap();
    match input(book).plan() {
        Err(PlanError::PriceResolution { expected_s, got_s }) => {
            assert_eq!((expected_s, got_s), (900, 3600));
        }
        other => panic!("expected a resolution refusal, got {other:?}"),
    }
}

#[test]
fn a_stale_forecast_is_refused() {
    let request = PlanInput::new(
        NOW,
        vec![state("a", 10.0, 2000.0, 50.0)],
        flat_book(NOW, HORIZON_SLOTS),
        settlement(),
    )
    .with_forecasts(
        forecasts(0.0, HORIZON_SLOTS),
        forecasts(0.5, HORIZON_SLOTS),
        NOW - 4000,
    );
    match request.plan() {
        Err(PlanError::StaleForecast { age_s, limit_s, .. }) => {
            assert_eq!((age_s, limit_s), (4000, 1800));
        }
        other => panic!("expected a stale forecast, got {other:?}"),
    }
}

#[test]
fn a_short_forecast_is_refused() {
    let request = PlanInput::new(
        NOW,
        vec![state("a", 10.0, 2000.0, 50.0)],
        flat_book(NOW, HORIZON_SLOTS),
        settlement(),
    )
    .with_forecasts(forecasts(0.0, 3), forecasts(0.5, HORIZON_SLOTS), NOW);
    assert!(matches!(
        request.plan(),
        Err(PlanError::ForecastLength { expected: 120, .. })
    ));
}

#[test]
fn the_plan_is_a_deterministic_120_slot_timeline() {
    let request = input(book(NOW, HORIZON_SLOTS, |index| {
        if index % 8 < 4 { 0.15 } else { 0.35 }
    }));
    let first = request.plan().unwrap();
    let second = request.plan().unwrap();
    assert_eq!(first, second);
    assert_eq!(first.slots.len(), HORIZON_SLOTS);
    assert_eq!(first.horizon_start, NOW);
    assert_eq!(first.horizon_end, NOW + 120 * 900);
    assert_eq!(first.slots[0].t_start, NOW);
    assert_eq!(first.slots[1].idx, 1);
    assert!(first.sell_floor.is_some());
    for slot in &first.slots {
        assert!(slot.battery_kw.is_finite());
        assert!(slot.soc_kwh.is_finite());
        assert_ne!(slot.reason, "");
    }
}
