//! The real plan source: horizon, refusals, fallbacks and the solve.

use std::sync::Mutex;

use energy_price::PriceBook;

use crate::dispatch::BoxFuture;
use crate::engine::PlanSource;
use crate::plan::{HORIZON_SLOTS, PackState, Plan, PlanError, SLOT_SECONDS};
use crate::plan_source::{MIN_HORIZON_SLOTS, PlanSupply, SourceConfig, Supply};
use crate::replan::Changes;

/// The engine's plan source over a [`PlanSupply`].
///
/// One plan reads the supply once, decides how much of the horizon the prices really cover,
/// refuses a stale book or an unusable measured history, builds the EWMA fallback when a provider
/// has no forecast, and solves. Nothing here reads a clock: `now` is the engine's.
pub struct LivePlanSource<S: PlanSupply> {
    supply: S,
    config: SourceConfig,
    pending: Mutex<Changes>,
    last: Mutex<Option<Fingerprint>>,
}

/// What makes a stored plan's inputs outdated without reading the inputs again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    coverage_end: i64,
    fetched_at: i64,
    forecast_as_of: Option<i64>,
}

/// The forecast the plan will actually be solved with.
#[derive(Debug, Clone, PartialEq)]
struct PlannedForecast {
    pv_kw: Vec<f64>,
    load_kw: Vec<f64>,
    as_of: i64,
    note: Option<String>,
}

impl<S: PlanSupply> LivePlanSource<S> {
    /// A source over `supply` with the given policy.
    pub fn new(supply: S, config: SourceConfig) -> Self {
        Self {
            supply,
            config,
            pending: Mutex::new(Changes::none()),
            last: Mutex::new(None),
        }
    }

    /// The policy in force.
    pub fn config(&self) -> &SourceConfig {
        &self.config
    }

    /// Build one plan from one supply read.
    fn assemble(&self, now: i64, packs: Vec<PackState>, supply: Supply) -> Result<Plan, PlanError> {
        self.refuse_stale_prices(now, supply.price_fetched_at)?;
        let slots = horizon_slots(&supply.prices, now)?;
        let from = supply.prices.slot_at(now).map_or(now, |slot| slot.t_start);
        let forecast = self.forecast(&supply, from, slots, now)?;
        let input = self
            .config
            .input(now, packs, supply.prices, supply.settlement)
            .with_horizon_slots(slots)
            .with_forecasts(forecast.pv_kw, forecast.load_kw, forecast.as_of);
        let input = match forecast.note {
            Some(note) => input.with_forecast_note(note),
            None => input,
        };
        input.plan()
    }

    /// Refuse a price book whose publication is older than the configured bound.
    fn refuse_stale_prices(&self, now: i64, fetched_at: i64) -> Result<(), PlanError> {
        let age_s = now.saturating_sub(fetched_at).max(0);
        if age_s > self.config.price_max_age_s {
            return Err(PlanError::StalePrices {
                age_s,
                limit_s: self.config.price_max_age_s,
            });
        }
        Ok(())
    }

    /// The freshest usable forecast, or the EWMA fallback (marked degraded).
    fn forecast(
        &self,
        supply: &Supply,
        from: i64,
        slots: usize,
        now: i64,
    ) -> Result<PlannedForecast, PlanError> {
        if let Some(forecast) = self.fresh_forecast(supply, slots, now) {
            return Ok(forecast);
        }
        let note = fallback_note(supply, slots, now, self.config.forecast_max_age_s);
        let fallback = supply
            .history
            .degraded_forecast(from, slots, now, note.clone())?;
        Ok(PlannedForecast {
            pv_kw: fallback.pv_kw,
            load_kw: fallback.load_kw,
            as_of: fallback.as_of,
            note: Some(note),
        })
    }

    /// The provider forecast when it covers the horizon and is fresh enough.
    fn fresh_forecast(&self, supply: &Supply, slots: usize, now: i64) -> Option<PlannedForecast> {
        let forecast = supply.forecast.as_ref()?;
        let fresh = now.saturating_sub(forecast.as_of) <= self.config.forecast_max_age_s;
        if !fresh || forecast.len() < slots || !forecast.is_finite() {
            return None;
        }
        let note = forecast.note.clone().or_else(|| {
            forecast
                .quality
                .is_degraded()
                .then(|| forecast.quality.as_str().to_owned())
        });
        Some(PlannedForecast {
            pv_kw: forecast.pv_kw[..slots].to_vec(),
            load_kw: forecast.load_kw[..slots].to_vec(),
            as_of: forecast.as_of,
            note,
        })
    }

    /// Record which inputs changed since the previous read, for the next trigger check.
    fn note_changes(&self, supply: &Supply) {
        let fingerprint = Fingerprint {
            coverage_end: supply.prices.coverage_end().unwrap_or(0),
            fetched_at: supply.price_fetched_at,
            forecast_as_of: supply.forecast.as_ref().map(|forecast| forecast.as_of),
        };
        let Ok(mut last) = self.last.lock() else {
            return;
        };
        if let Some(previous) = *last {
            let prices = previous.fetched_at != fingerprint.fetched_at
                || previous.coverage_end != fingerprint.coverage_end;
            let forecast = previous.forecast_as_of != fingerprint.forecast_as_of;
            if (prices || forecast)
                && let Ok(mut pending) = self.pending.lock()
            {
                pending.prices |= prices;
                pending.forecast |= forecast;
            }
        }
        *last = Some(fingerprint);
    }
}

impl<S: PlanSupply> PlanSource for LivePlanSource<S> {
    fn plan(&self, now: i64, packs: Vec<PackState>) -> BoxFuture<'_, Result<Plan, PlanError>> {
        Box::pin(async move {
            let supply = self.supply.supply(now).await?;
            self.note_changes(&supply);
            self.assemble(now, packs, supply)
        })
    }

    fn changes(&self) -> Changes {
        self.pending.lock().map_or_else(
            |_| Changes::none(),
            |mut pending| std::mem::take(&mut *pending),
        )
    }
}

/// Contiguous quarter-hours the book covers from the slot containing `now`, capped at the horizon.
///
/// Never forward-fills: before the ~13:00 CET publication tomorrow's slots simply do not exist, so
/// the plan is shorter and the replan trigger replaces it once they do. A book that cannot cover
/// the next hour at all is refused as stale/insufficient instead of producing a one-slot plan.
fn horizon_slots(prices: &PriceBook, now: i64) -> Result<usize, PlanError> {
    let first = prices
        .slot_at(now)
        .ok_or(PlanError::PriceGap { t_start: now })?;
    if first.source.slot_seconds() != SLOT_SECONDS {
        return Err(PlanError::PriceResolution {
            expected_s: SLOT_SECONDS,
            got_s: first.source.slot_seconds(),
        });
    }
    let mut slots = 0;
    let mut t_start = first.t_start;
    while slots < HORIZON_SLOTS {
        let covered = prices
            .slot_at(t_start)
            .is_some_and(|slot| slot.t_start == t_start);
        if !covered {
            break;
        }
        slots += 1;
        t_start = t_start.saturating_add(SLOT_SECONDS);
    }
    if slots < MIN_HORIZON_SLOTS {
        return Err(PlanError::PriceCoverage {
            slots,
            minimum: MIN_HORIZON_SLOTS,
        });
    }
    Ok(slots)
}

/// Why the provider forecast was not used.
fn fallback_note(supply: &Supply, slots: usize, now: i64, max_age_s: i64) -> String {
    match &supply.forecast {
        None => "no forecast available; EWMA fallback from measured history".to_owned(),
        Some(forecast) if forecast.len() < slots => format!(
            "forecast covers {} of {slots} slots; EWMA fallback",
            forecast.len()
        ),
        Some(forecast) => {
            let age = now.saturating_sub(forecast.as_of).max(0);
            format!("forecast is {age}s old (limit {max_age_s}s); EWMA fallback")
        }
    }
}
