//! The scenario builder: everything a solve needs, and nothing it has to guess.

use energy_price::{PriceBook, Settlement};

use crate::plan::{HORIZON_SLOTS, PackState, Plan, PlanError, assemble};

/// A plan request under construction.
///
/// The defaults are the ones the add-on shipped: no reserve above the pack floor, no wear, no
/// terminal target, and a 15-minute replan with a 30-minute stale bound. Forecasts have no timestamp
/// until [`with_forecasts`](PlanInput::with_forecasts) supplies one, so an unset forecast is refused
/// rather than planned as zero.
#[derive(Debug, Clone)]
pub struct PlanInput {
    /// Plan instant, UTC epoch seconds.
    pub now: i64,
    /// The fleet and its SoC.
    pub packs: Vec<PackState>,
    /// The validated price book; gaps are already refused at ingest.
    pub prices: PriceBook,
    /// Slots this plan covers, at most [`HORIZON_SLOTS`].
    ///
    /// The horizon is shortened (never forward-filled) when tomorrow's prices are not published
    /// yet, so the solver only ever sees prices that really exist. Everything else (forecasts)
    /// must match this length exactly.
    pub horizon_slots: usize,
    /// The settlement that prices feed-in and the sell floor.
    pub settlement: Settlement,
    /// PV forecast, kW per slot, exactly `horizon_slots` values.
    pub pv_kw: Vec<f64>,
    /// Load forecast, kW per slot, exactly `horizon_slots` values.
    pub load_kw: Vec<f64>,
    /// When the forecasts were captured, UTC epoch seconds.
    pub forecast_as_of: Option<i64>,
    /// Why the forecast is degraded (a fallback was used), `None` when it is fresh.
    pub forecast_note: Option<String>,
    /// Per-pack reserve floor, percent.
    pub reserve_soc_pct: f64,
    /// Wear cost per kWh delivered, EUR/kWh.
    pub wear_eur_per_kwh: f64,
    /// Terminal target, percent of the fleet's usable ceiling.
    pub terminal_target_pct: Option<f64>,
    /// Penalty per kWh of terminal-target error, EUR/kWh.
    pub target_penalty_eur_per_kwh: f64,
    /// Normal replan interval, seconds.
    pub replan_seconds: i64,
    /// Absolute stale-plan bound, seconds.
    pub stale_plan_seconds: i64,
}

impl PlanInput {
    /// A request for the fleet at `now` with the given price book and settlement.
    pub fn new(now: i64, packs: Vec<PackState>, prices: PriceBook, settlement: Settlement) -> Self {
        Self {
            now,
            packs,
            prices,
            horizon_slots: HORIZON_SLOTS,
            settlement,
            pv_kw: Vec::new(),
            load_kw: Vec::new(),
            forecast_as_of: None,
            forecast_note: None,
            reserve_soc_pct: 0.0,
            wear_eur_per_kwh: 0.0,
            terminal_target_pct: None,
            target_penalty_eur_per_kwh: 1.0,
            replan_seconds: 900,
            stale_plan_seconds: 1800,
        }
    }

    /// Attach the PV/load forecasts and the instant they were captured.
    #[must_use]
    pub fn with_forecasts(mut self, pv_kw: Vec<f64>, load_kw: Vec<f64>, as_of: i64) -> Self {
        self.pv_kw = pv_kw;
        self.load_kw = load_kw;
        self.forecast_as_of = Some(as_of);
        self
    }

    /// Shorten the horizon to `slots` quarter-hours (1..=[`HORIZON_SLOTS`]).
    #[must_use]
    pub fn with_horizon_slots(mut self, slots: usize) -> Self {
        self.horizon_slots = slots;
        self
    }

    /// Mark the forecasts as degraded, with the reason the fallback was used.
    #[must_use]
    pub fn with_forecast_note(mut self, note: impl Into<String>) -> Self {
        self.forecast_note = Some(note.into());
        self
    }

    /// Set the per-pack reserve floor above the configured minimum SoC.
    #[must_use]
    pub fn with_reserve(mut self, reserve_soc_pct: f64) -> Self {
        self.reserve_soc_pct = reserve_soc_pct;
        self
    }

    /// Set the wear cost per delivered kWh.
    #[must_use]
    pub fn with_wear(mut self, wear_eur_per_kwh: f64) -> Self {
        self.wear_eur_per_kwh = wear_eur_per_kwh;
        self
    }

    /// Set the terminal target as a percentage of the fleet's usable ceiling.
    #[must_use]
    pub fn with_terminal_target_pct(mut self, pct: f64) -> Self {
        self.terminal_target_pct = Some(pct);
        self
    }

    /// Set the replan and stale-plan bounds.
    #[must_use]
    pub fn with_staleness(mut self, replan_seconds: i64, stale_plan_seconds: i64) -> Self {
        self.replan_seconds = replan_seconds;
        self.stale_plan_seconds = stale_plan_seconds;
        self
    }

    /// Validate, solve and return the plan -- or the typed reason it was refused.
    pub fn plan(&self) -> Result<Plan, PlanError> {
        assemble::build(self)
    }
}
