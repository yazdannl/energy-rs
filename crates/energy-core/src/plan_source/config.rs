//! The plan source's policy: the solver knobs and the two staleness bounds.

use energy_price::{PriceBook, Settlement};

use crate::plan::{PackState, PlanInput};

/// What the plan source adds to the price book and the forecasts.
///
/// The defaults are the shipped ones: no reserve, no wear term, no terminal target, a 15-minute
/// replan with a 30-minute stale bound, six hours before a price publication is refused as stale
/// (Zonneplan publishes roughly once a day) and two replans before a forecast is replaced by the
/// EWMA fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceConfig {
    /// Per-pack reserve floor above the pack minimum, percent.
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
    /// Oldest price publication the planner still accepts, seconds.
    pub price_max_age_s: i64,
    /// Oldest forward forecast the planner still accepts, seconds.
    pub forecast_max_age_s: i64,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            reserve_soc_pct: 0.0,
            wear_eur_per_kwh: 0.0,
            terminal_target_pct: None,
            target_penalty_eur_per_kwh: 1.0,
            replan_seconds: 900,
            stale_plan_seconds: 1800,
            price_max_age_s: 6 * 3600,
            forecast_max_age_s: 1800,
        }
    }
}

impl SourceConfig {
    /// The request under construction, with this policy folded in.
    pub(crate) fn input(
        &self,
        now: i64,
        packs: Vec<PackState>,
        prices: PriceBook,
        settlement: Settlement,
    ) -> PlanInput {
        let mut input = PlanInput::new(now, packs, prices, settlement)
            .with_reserve(self.reserve_soc_pct)
            .with_wear(self.wear_eur_per_kwh)
            .with_staleness(self.replan_seconds, self.stale_plan_seconds);
        if let Some(pct) = self.terminal_target_pct {
            input = input.with_terminal_target_pct(pct);
        }
        input.target_penalty_eur_per_kwh = self.target_penalty_eur_per_kwh;
        input
    }
}
