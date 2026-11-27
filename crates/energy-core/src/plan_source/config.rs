//! The plan source's policy: the solver knobs and the two staleness bounds.

use energy_price::{PriceBook, Settlement};

use crate::plan::{PackState, PlanInput, Policy};

/// What the plan source adds to the price book and the forecasts.
///
/// The defaults are the shipped ones: no reserve, no wear term, no terminal target, a 15-minute
/// replan with a 30-minute stale bound, six hours before a price publication is refused as stale
/// (Zonneplan publishes roughly once a day) and two replans before a forecast is replaced by the
/// EWMA fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceConfig {
    /// Hard SoC floor, percent of each pack's capacity, never below its own `min_soc_pct`.
    ///
    /// The planner raises its aggregate `e_min_kwh` to this value, and the dispatcher's clamp
    /// ladder uses the same number to refuse discharges below it: planning deeper would schedule
    /// energy the ladder will not deliver. It is distinct from `Policy::peak_reserve_pct` (a soft
    /// end-of-horizon guideline) and `Policy::solar_reserve_pct` (headroom kept free of grid
    /// energy); neither of those is a hard floor. The server copies the operator's stored setting
    /// here when it builds this config at boot.
    pub reserve_soc_pct: f64,
    /// Wear cost per kWh delivered, EUR/kWh.
    pub wear_eur_per_kwh: f64,
    /// The post-net-metering policy (export gate, reserves, basis).
    pub policy: Policy,
    /// Terminal target, percent of the fleet's usable ceiling.
    pub terminal_target_pct: Option<f64>,
    /// Penalty per kWh of terminal-target error, EUR/kWh.
    pub target_penalty_eur_per_kwh: f64,
    /// Number of slots after the current one that the measured-load bias may move (0..=96).
    pub load_forecast_bias_slots: usize,
    /// Consumption lookback for the bias, seconds; 0 resolves to `replan_seconds`.
    pub load_forecast_bias_lookback_s: i64,
    /// Normal replan interval, seconds.
    pub replan_seconds: i64,
    /// Absolute stale-plan bound, seconds.
    pub stale_plan_seconds: i64,
    /// Oldest price publication the planner still accepts, seconds.
    pub price_max_age_s: i64,
    /// Oldest forward forecast the planner still accepts, seconds.
    pub forecast_max_age_s: i64,
    /// Grid import budget for maintenance charge, W (0 = the pack limits decide).
    pub maintenance_budget_w: f64,
    /// Fleet charge ceiling for maintenance, W (0 = the pack limits decide).
    pub maintenance_fleet_cap_w: f64,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            reserve_soc_pct: 0.0,
            wear_eur_per_kwh: 0.0,
            policy: Policy::legacy(),
            terminal_target_pct: None,
            target_penalty_eur_per_kwh: 1.0,
            load_forecast_bias_slots: 1,
            load_forecast_bias_lookback_s: 0,
            replan_seconds: 900,
            stale_plan_seconds: 1800,
            price_max_age_s: 6 * 3600,
            forecast_max_age_s: 1800,
            maintenance_budget_w: 0.0,
            maintenance_fleet_cap_w: 0.0,
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
        input.policy = self.policy;
        input
    }
}
