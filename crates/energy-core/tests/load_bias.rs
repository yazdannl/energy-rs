//! The replan's near-term load forecast follows consumption actually observed between solves.

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
use energy_core::plan::{HORIZON_SLOTS, PlanError};
use energy_core::plan_source::{
    Forecast, LivePlanSource, MeasuredHistory, PlanSupply, SourceConfig, Supply,
};
use support::{NOW, flat_book, settlement, state};

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

/// A source that refuses its first solve, so a failed plan cannot clear the observation window.
struct RefuseOnce<S> {
    inner: S,
    refused: Mutex<bool>,
}

impl<S: PlanSupply> PlanSupply for RefuseOnce<S> {
    fn supply(&self, now: i64) -> BoxFuture<'_, Result<Supply, PlanError>> {
        Box::pin(async move {
            let refuse = {
                let mut refused = self.refused.lock().expect("flag");
                let first = !*refused;
                *refused = true;
                first
            };
            if refuse {
                return Err(PlanError::StalePrices {
                    age_s: 99_999,
                    limit_s: 21_600,
                });
            }
            self.inner.supply(now).await
        })
    }
}

fn supply() -> Supply {
    Supply {
        history: MeasuredHistory::new(
            (0..192).map(|index| NOW - 900 * (192 - index)).collect(),
            vec![0.4; 192],
            vec![1.0; 192],
        )
        .expect("history"),
        forecast: Some(Forecast::fresh(
            vec![0.0; HORIZON_SLOTS],
            vec![3.5; HORIZON_SLOTS],
            NOW,
        )),
        prices: flat_book(NOW, HORIZON_SLOTS),
        price_fetched_at: NOW,
        settlement: settlement(),
    }
}

fn config(slots: usize, lookback_s: i64, replan_s: i64) -> SourceConfig {
    SourceConfig {
        load_forecast_bias_slots: slots,
        load_forecast_bias_lookback_s: lookback_s,
        replan_seconds: replan_s,
        ..SourceConfig::default()
    }
}

fn source(supply: Supply, config: SourceConfig) -> LivePlanSource<FakeSupply> {
    LivePlanSource::new(FakeSupply::new(supply), config)
}

fn packs() -> Vec<energy_core::plan::PackState> {
    vec![state("a", 1.92, 800.0, 50.0)]
}

/// A five-minute run at a steady load, sampled every half minute.
fn observe<S: PlanSupply>(source: &LivePlanSource<S>, from: i64, to: i64, load_kw: f64) {
    for at in (from..=to).step_by(30) {
        source.observe_load(at, load_kw);
    }
}

#[tokio::test]
async fn five_minutes_of_consumption_corrects_the_current_and_next_slot() {
    let src = source(supply(), config(1, 0, 300));
    observe(&src, NOW - 300, NOW, 1.2);
    let plan = src.plan(NOW, packs()).await.expect("plan");
    assert!((plan.slots[0].load_kw - 1.2).abs() < 1e-6);
    assert!((plan.slots[1].load_kw - 1.2).abs() < 1e-6);
    assert_eq!(plan.slots[2].load_kw, 3.5, "slot N+1 is untouched");
    let note = plan.degraded.expect("note");
    assert!(note.contains("1.200 kW"), "{note}");
    assert!(
        note.contains("lookback 300 s from replan_seconds"),
        "{note}"
    );
    assert!(note.contains("2 slot(s)"), "{note}");
}

#[tokio::test]
async fn zero_bias_slots_corrects_only_the_current_slot() {
    let src = source(supply(), config(0, 0, 300));
    observe(&src, NOW - 120, NOW, 1.2);
    let plan = src.plan(NOW, packs()).await.expect("plan");
    assert!((plan.slots[0].load_kw - 1.2).abs() < 1e-6);
    assert_eq!(plan.slots[1].load_kw, 3.5);
    assert!(plan.degraded.expect("note").contains("1 slot(s)"));
}

#[tokio::test]
async fn short_negative_and_non_finite_evidence_never_biases_the_forecast() {
    let src = source(supply(), config(1, 0, 300));
    src.observe_load(NOW - 30, 1.2);
    src.observe_load(NOW, 1.2);
    let plan = src.plan(NOW, packs()).await.expect("short plan");
    assert_eq!(plan.slots[0].load_kw, 3.5);
    assert!(plan.degraded.is_none());

    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        let src = source(supply(), config(1, 0, 300));
        observe(&src, NOW - 300, NOW - 30, 1.2);
        src.observe_load(NOW, bad);
        let plan = src.plan(NOW, packs()).await.expect("bad sample plan");
        assert_eq!(plan.slots[0].load_kw, 3.5, "{bad}");
        assert!(plan.degraded.is_none(), "{bad}");
    }
}

#[tokio::test]
async fn a_refused_solve_keeps_the_observation_window() {
    let inner = FakeSupply::new(supply());
    let src = LivePlanSource::new(
        RefuseOnce {
            inner,
            refused: Mutex::new(false),
        },
        config(1, 0, 300),
    );
    observe(&src, NOW - 300, NOW, 1.2);
    assert!(
        src.plan(NOW, packs()).await.is_err(),
        "the first solve refuses"
    );
    let plan = src.plan(NOW, packs()).await.expect("accepted plan");
    assert!((plan.slots[0].load_kw - 1.2).abs() < 1e-6);
    assert!(plan.degraded.expect("note").contains("last solve none"));
}

#[tokio::test]
async fn an_explicit_lookback_reaches_past_the_replan_interval() {
    // The last two minutes are quiet; the earlier four are busy. A 60 s replan window sees only
    // the quiet part, while an explicit 600 s window still includes the busy samples.
    let src = source(supply(), config(1, 0, 60));
    observe(&src, NOW - 300, NOW - 120, 4.0);
    observe(&src, NOW - 90, NOW, 1.0);
    let short = src.plan(NOW, packs()).await.expect("short plan");
    assert!((short.slots[0].load_kw - 1.0).abs() < 1e-6);

    let src = source(supply(), config(1, 600, 60));
    observe(&src, NOW - 300, NOW - 120, 4.0);
    observe(&src, NOW - 90, NOW, 1.0);
    let long = src.plan(NOW, packs()).await.expect("long plan");
    assert!(long.slots[0].load_kw > 1.5, "{}", long.slots[0].load_kw);
    assert!(
        long.degraded
            .expect("note")
            .contains("lookback 600 s from setting")
    );
}

#[tokio::test]
async fn a_bias_is_clamped_to_a_non_negative_load_forecast() {
    let mut scripted = supply();
    scripted.forecast = Some(Forecast::fresh(
        vec![0.0; HORIZON_SLOTS],
        vec![0.2; HORIZON_SLOTS],
        NOW,
    ));
    let src = source(scripted, config(1, 0, 300));
    observe(&src, NOW - 120, NOW, 0.0);
    let plan = src.plan(NOW, packs()).await.expect("plan");
    assert_eq!(plan.slots[0].load_kw, 0.0);
    assert!(plan.slots.iter().all(|slot| slot.load_kw.is_finite()));
    assert!(plan.degraded.expect("note").contains("0.000 kW"));
}
