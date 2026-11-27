//! The real plan source: horizon, refusals, fallbacks and the solve.

use std::sync::{Arc, Mutex};

use super::load_bias::{apply_bias, effective_lookback_s, measure};
use crate::dispatch::BoxFuture;
use crate::engine::PlanSource;
use crate::maintenance::MaintenancePort;
use crate::plan::{Differential, PackState, Plan, PlanError, SLOT_SECONDS};
use crate::plan_source::{PlanSupply, PolicyHandle, SourceConfig, Supply};
use crate::replan::Changes;

/// The engine's plan source over a [`PlanSupply`].
///
/// One plan reads the supply once, decides how much of the horizon the prices really cover,
/// refuses a stale book or an unusable measured history, builds the EWMA fallback when a provider
/// has no forecast, and solves. Nothing here reads a clock: `now` is the engine's.
pub struct LivePlanSource<S: PlanSupply> {
    policy: Option<Arc<PolicyHandle>>,
    supply: S,
    config: SourceConfig,
    pending: Mutex<Changes>,
    last: Mutex<Option<Fingerprint>>,
    differential: Differential,
    maintenance: Option<Arc<MaintenancePort>>,
    /// Time-stamped live load samples, kept for at most an hour.
    samples: Mutex<Vec<(i64, f64)>>,
    /// Last accepted solve, for the bias note only; samples survive it on purpose.
    last_solve_at: Mutex<Option<i64>>,
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
            policy: None,
            supply,
            config,
            pending: Mutex::new(Changes::none()),
            last: Mutex::new(None),
            differential: Differential::default(),
            maintenance: None,
            samples: Mutex::new(Vec::new()),
            last_solve_at: Mutex::new(None),
        }
    }

    /// Follow a live policy (settings plus the measured basis).
    #[must_use]
    pub fn with_policy(mut self, policy: Arc<PolicyHandle>) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Attach the price-aware full-charge rule.
    #[must_use]
    pub fn with_maintenance(mut self, maintenance: Arc<MaintenancePort>) -> Self {
        self.maintenance = Some(maintenance);
        self
    }

    /// The import and fleet ceilings the maintenance planner must stay below.
    #[must_use]
    pub fn with_maintenance_budget(mut self, grid_w: f64, fleet_w: f64) -> Self {
        self.config.maintenance_budget_w = grid_w;
        self.config.maintenance_fleet_cap_w = fleet_w;
        self
    }

    /// The policy in force.
    pub fn config(&self) -> &SourceConfig {
        &self.config
    }

    /// Build one plan from one supply read.
    fn assemble(&self, now: i64, packs: Vec<PackState>, supply: Supply) -> Result<Plan, PlanError> {
        self.refuse_stale_prices(now, supply.price_fetched_at)?;
        let slots = super::live_support::horizon_slots(&supply.prices, now)?;
        let from = supply.prices.slot_at(now).map_or(now, |slot| slot.t_start);
        let mut forecast = self.forecast(&supply, from, slots, now)?;
        if let Some(note) = self.load_bias(&mut forecast, now) {
            forecast.note = Some(match forecast.note.take() {
                Some(prior) => format!("{prior}; {note}"),
                None => note,
            });
        }
        let maintenance_packs = packs.clone();
        let mut input = self
            .config
            .input(now, packs, supply.prices, supply.settlement)
            .with_horizon_slots(slots)
            .with_forecasts(forecast.pv_kw, forecast.load_kw, forecast.as_of);
        if let Some(policy) = &self.policy {
            input.policy = policy.get();
        }
        // The net-metering regime is the operator's switch, and it prices this plan: apply it to
        // the settlement the policy resolved to, whatever source that policy came from.
        input.settlement.set_regime(input.policy.net_metering);
        let input = match forecast.note {
            Some(note) => input.with_forecast_note(note),
            None => input,
        };
        let check = self.differential.due(now).then_some(&self.differential);
        let mut plan = input.plan_checked(check)?;
        if let Ok(mut anchor) = self.last_solve_at.lock() {
            *anchor = Some(now);
        }
        self.place_maintenance(now, &maintenance_packs, &mut plan);
        Ok(plan)
    }

    /// Fold due packs' cheap-slot charges into the solved plan.
    fn place_maintenance(&self, now: i64, packs: &[PackState], plan: &mut Plan) {
        let Some(port) = &self.maintenance else {
            return;
        };
        let budget = self.maintenance_budget(packs);
        let candidates = crate::maintenance::candidates(plan, now, budget);
        let selection = port.assignments(now, packs, &candidates, budget);
        let capacity: f64 = packs.iter().map(|pack| pack.spec.capacity_kwh).sum();
        let eta = super::live_support::weighted_charge_efficiency(packs);
        crate::maintenance::apply(plan, &selection.assignments, capacity, eta);
        plan.maintenance = selection.assignments;
        plan.maintenance_unassigned = selection.unassigned;
    }

    /// The smaller of the grid-import budget and the fleet charge ceiling.
    fn maintenance_budget(&self, packs: &[PackState]) -> f64 {
        let pack_cap: f64 = packs.iter().map(|pack| pack.spec.max_charge_w).sum();
        let mut budget = pack_cap.max(0.0);
        if self.config.maintenance_budget_w > 0.0 {
            budget = budget.min(self.config.maintenance_budget_w);
        }
        if self.config.maintenance_fleet_cap_w > 0.0 {
            budget = budget.min(self.config.maintenance_fleet_cap_w);
        }
        budget
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
        let note =
            super::live_support::fallback_note(supply, slots, now, self.config.forecast_max_age_s);
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

    /// Bias the near-term load forecast from the measured consumption window.
    fn load_bias(&self, forecast: &mut PlannedForecast, now: i64) -> Option<String> {
        let setting = self.config.load_forecast_bias_lookback_s;
        let lookback_s = effective_lookback_s(setting, self.config.replan_seconds);
        let samples = self.samples.lock().ok()?;
        let observed = measure(&samples, now, lookback_s)?;
        let (bias_kw, slots) = apply_bias(
            &mut forecast.load_kw,
            observed.rate_kw,
            self.config.load_forecast_bias_slots,
        )?;
        drop(samples);
        let anchor = self
            .last_solve_at
            .lock()
            .ok()?
            .map_or_else(|| "none".to_owned(), |at| at.to_string());
        let source = if setting > 0 {
            "setting"
        } else {
            "replan_seconds"
        };
        Some(format!(
            "load bias: {:.3} kW over {} s (lookback {} s from {}, last solve {}), {:.3} kW on {} slot(s)",
            observed.rate_kw, observed.elapsed_s, lookback_s, source, anchor, bias_kw, slots
        ))
    }

    /// Record which inputs changed since the previous read, for the next trigger check.
    ///
    /// A forecast's `as_of` is the read instant, and the real supply rebuilds its series (the EWMA
    /// fallback especially) on every read. Compared raw it changes on every tick, which turns the
    /// "refreshed inputs" trigger into a replan every five seconds. The capture instant is bucketed
    /// to the clock slot instead: an unchanged forecast then only forces the replan the slot
    /// boundary already asks for.
    fn note_changes(&self, supply: &Supply) {
        let fingerprint = Fingerprint {
            coverage_end: supply.prices.coverage_end().unwrap_or(0),
            fetched_at: supply.price_fetched_at,
            forecast_as_of: supply
                .forecast
                .as_ref()
                .map(|forecast| forecast.as_of.div_euclid(SLOT_SECONDS)),
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
    fn observe_load(&self, now: i64, load_kw: f64) {
        super::load_bias::record_sample(&self.samples, now, load_kw);
    }

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

    fn maintenance(&self) -> Option<&MaintenancePort> {
        self.maintenance.as_deref()
    }
}
