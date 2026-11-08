//! The real plan source: normal, pre-publication, refusal and fallback edges.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::sync::Mutex;

use energy_core::dispatch::BoxFuture;
use energy_core::engine::PlanSource;
use energy_core::plan::{HORIZON_SLOTS, Plan, PlanError};
use energy_core::plan_source::{
    Forecast, LivePlanSource, MeasuredHistory, PlanSupply, SourceConfig, Supply,
};
use energy_price::PriceBook;
use support::{NOW, book, flat_book, settlement, state};

/// A supply that hands the same scripted read to every call.
struct FakeSupply {
    supply: Mutex<Supply>,
}

impl FakeSupply {
    fn new(supply: Supply) -> Self {
        Self {
            supply: Mutex::new(supply),
        }
    }
}

impl PlanSupply for FakeSupply {
    fn supply(&self, _now: i64) -> BoxFuture<'_, Result<Supply, PlanError>> {
        Box::pin(async move { Ok(self.supply.lock().expect("supply").clone()) })
    }
}

/// Two days of contiguous quarter-hours ending one slot before `NOW`, all sunny and busy.
fn history() -> MeasuredHistory {
    let slots = 192;
    let starts: Vec<i64> = (0..slots)
        .map(|index| NOW - 900 * (i64::try_from(slots - index).unwrap()))
        .collect();
    MeasuredHistory::new(starts, vec![1.0; slots], vec![0.5; slots]).expect("history")
}

fn fresh_forecast() -> Forecast {
    Forecast::fresh(vec![0.4; HORIZON_SLOTS], vec![0.6; HORIZON_SLOTS], NOW)
}

fn supply(prices: PriceBook, fetched_at: i64) -> Supply {
    Supply {
        history: history(),
        forecast: Some(fresh_forecast()),
        prices,
        price_fetched_at: fetched_at,
        settlement: settlement(),
    }
}

fn source(supply: Supply) -> LivePlanSource<FakeSupply> {
    LivePlanSource::new(FakeSupply::new(supply), SourceConfig::default())
}

fn packs() -> Vec<energy_core::plan::PackState> {
    vec![state("a", 1.92, 800.0, 50.0)]
}

/// One plan, or a panic with the refusal.
async fn plan(source: &LivePlanSource<FakeSupply>) -> Plan {
    source.plan(NOW, packs()).await.expect("plan")
}

#[tokio::test]
async fn a_full_book_plans_the_whole_horizon() {
    let source = source(supply(flat_book(NOW, HORIZON_SLOTS), NOW));
    let plan = plan(&source).await;
    assert_eq!(plan.slots.len(), HORIZON_SLOTS);
    assert_eq!(plan.horizon_start, NOW);
    assert_eq!(plan.horizon_end, NOW + 120 * 900);
    assert_eq!(plan.slots[0].pv_kw, 0.4);
    assert_eq!(plan.slots[0].load_kw, 0.6);
    assert!(plan.degraded.is_none());
    let floor = plan.sell_floor.expect("floor");
    assert!((floor - 0.10).abs() < 1e-9, "floor {floor}");
    for slot in &plan.slots {
        assert!(slot.battery_kw.is_finite());
    }
}

#[tokio::test]
async fn before_the_publication_the_horizon_is_shortened() {
    let source = source(supply(
        book(NOW, 40, |index| 0.10 + index as f64 / 1000.0),
        NOW,
    ));
    let plan = plan(&source).await;
    assert_eq!(plan.slots.len(), 40, "only the published slots are planned");
    assert_eq!(plan.horizon_end, NOW + 40 * 900);
    assert!((plan.slots[39].buy_all_in - 0.139).abs() < 1e-9);
    assert_eq!(plan.slots[39].pv_kw, 0.4);
}

#[tokio::test]
async fn a_book_below_the_minimum_horizon_is_refused() {
    let source = source(supply(flat_book(NOW, 2), NOW));
    let result = source.plan(NOW, packs()).await;
    match result {
        Err(PlanError::PriceCoverage { slots, minimum }) => {
            assert_eq!((slots, minimum), (2, 4));
        }
        other => panic!("expected a coverage refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_statistics_gap_is_refused_rather_than_zero_filled() {
    let mut starts: Vec<i64> = (0..96).map(|index| NOW - 900 * (96 - index)).collect();
    starts[40] += 900;
    let result = MeasuredHistory::new(starts, vec![1.0; 96], vec![0.5; 96]);
    match result {
        Err(PlanError::StatsGap { series, t_start }) => {
            assert_eq!(series, "pv/load");
            assert_eq!(t_start, NOW - 900 * 55);
        }
        other => panic!("expected a statistics gap, got {other:?}"),
    }
}

#[tokio::test]
async fn a_stale_price_publication_is_refused() {
    let source = source(supply(flat_book(NOW, HORIZON_SLOTS), NOW - 7 * 3600));
    let result = source.plan(NOW, packs()).await;
    match result {
        Err(PlanError::StalePrices { age_s, limit_s }) => {
            assert_eq!((age_s, limit_s), (7 * 3600, 6 * 3600));
        }
        other => panic!("expected a stale price refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_forecast_falls_back_to_ewma_and_is_marked_degraded() {
    let mut read = supply(flat_book(NOW, HORIZON_SLOTS), NOW);
    read.forecast = None;
    let source = source(read);
    let plan = plan(&source).await;
    let degraded = plan.degraded.expect("degraded note");
    assert!(degraded.contains("EWMA"), "note {degraded}");
    assert_eq!(plan.slots.len(), HORIZON_SLOTS);
    assert_eq!(plan.slots[0].pv_kw, 1.0, "the EWMA of a flat history");
    assert_eq!(plan.slots[0].load_kw, 0.5);
}

#[tokio::test]
async fn a_stale_forecast_is_replaced_by_the_fallback() {
    let mut read = supply(flat_book(NOW, HORIZON_SLOTS), NOW);
    read.forecast = Some(Forecast::fresh(
        vec![0.9; HORIZON_SLOTS],
        vec![0.1; HORIZON_SLOTS],
        NOW - 4000,
    ));
    let source = source(read);
    let plan = plan(&source).await;
    assert!(plan.degraded.is_some());
    assert_eq!(plan.slots[0].pv_kw, 1.0);
    assert_eq!(plan.slots[0].load_kw, 0.5);
}

#[tokio::test]
async fn the_same_supply_produces_the_same_plan() {
    let source = source(supply(
        book(
            NOW,
            HORIZON_SLOTS,
            |index| {
                if index % 8 < 4 { 0.15 } else { 0.35 }
            },
        ),
        NOW,
    ));
    assert_eq!(plan(&source).await, plan(&source).await);
}

/// A supply that rebuilds its forecast on every read, exactly as the real one does (`as_of = now`).
struct ChurningSupply {
    supply: Supply,
}

impl PlanSupply for ChurningSupply {
    fn supply(&self, now: i64) -> BoxFuture<'_, Result<Supply, PlanError>> {
        Box::pin(async move {
            let mut supply = self.supply.clone();
            if let Some(forecast) = supply.forecast.as_mut() {
                forecast.as_of = now;
            }
            Ok(supply)
        })
    }
}

/// A forecast whose only change is its capture instant must not force a replan on every tick.
#[tokio::test]
async fn a_forecast_rebuilt_on_every_read_does_not_ask_for_a_replan() {
    let source = LivePlanSource::new(
        ChurningSupply {
            supply: supply(flat_book(NOW, HORIZON_SLOTS), NOW),
        },
        SourceConfig::default(),
    );
    source.plan(NOW, packs()).await.expect("first plan");
    let _ = source.changes();
    source.plan(NOW + 5, packs()).await.expect("second plan");
    let changes = source.changes();
    assert!(
        !changes.forecast,
        "the rebuilt-but-unchanged forecast asked for a replan"
    );
    // The next slot is a real change of the capture instant's bucket, and the trigger says so.
    source.plan(NOW + 900, packs()).await.expect("third plan");
    assert!(source.changes().forecast, "a new slot is a forecast change");
}
