//! Why a plan was refused. Every variant is a hold, never a zero-filled plan.

use thiserror::Error;

use energy_opt::SolveError;
use energy_price::PriceError;

/// A typed plan refusal.
#[derive(Debug, Error)]
pub enum PlanError {
    /// No pack is configured, so there is nothing to plan.
    #[error("no packs configured")]
    NoPacks,
    /// A pack's limits cannot produce a finite plan.
    #[error("pack {pack_id}: {issue}")]
    InvalidPack {
        /// The offending pack.
        pack_id: String,
        /// What is wrong.
        issue: &'static str,
    },
    /// A pack has no usable state of charge.
    #[error("pack {pack_id} has no usable state of charge")]
    MissingSoc {
        /// The offending pack.
        pack_id: String,
    },
    /// The forecasts do not cover the requested horizon.
    #[error("forecast {name} has {got} slots; the horizon needs {expected}")]
    ForecastLength {
        /// Which series.
        name: &'static str,
        /// Slots needed.
        expected: usize,
        /// Slots supplied.
        got: usize,
    },
    /// A forecast value is not finite.
    #[error("forecast {name}[{index}] is not finite")]
    ForecastNotFinite {
        /// Which series.
        name: &'static str,
        /// The bad index.
        index: usize,
    },
    /// No forecast timestamp was supplied.
    #[error("forecast {name} has no capture timestamp")]
    ForecastMissing {
        /// Which series.
        name: &'static str,
    },
    /// The forecast is older than the plan staleness bound.
    #[error("forecast {name} is stale: {age_s}s old, limit {limit_s}s")]
    StaleForecast {
        /// Which series.
        name: &'static str,
        /// Age at plan time.
        age_s: i64,
        /// Allowed age.
        limit_s: i64,
    },
    /// The price book does not cover a slot in the horizon.
    #[error("prices do not cover slot {t_start}")]
    PriceGap {
        /// The missing slot start.
        t_start: i64,
    },
    /// The price book resolution is not the planner's 15 minutes.
    #[error("prices are {got_s}s slots; the planner needs {expected_s}s")]
    PriceResolution {
        /// Needed resolution.
        expected_s: i64,
        /// Supplied resolution.
        got_s: i64,
    },
    /// The terminal target percentage is outside `0..=100`.
    #[error("terminal target {pct}% is outside 0..=100")]
    TerminalTargetPct {
        /// The configured percentage.
        pct: f64,
    },
    /// The terminal target lies outside the usable energy band.
    #[error("terminal target {target_kwh} kWh is outside [{e_min_kwh}, {e_max_kwh}]")]
    TerminalTargetRange {
        /// Target energy.
        target_kwh: f64,
        /// Lowest usable energy.
        e_min_kwh: f64,
        /// Highest usable energy.
        e_max_kwh: f64,
    },
    /// The requested horizon is not a usable number of slots.
    #[error("horizon of {slots} slots is outside 1..=120")]
    HorizonSlots {
        /// Slots requested.
        slots: usize,
    },
    /// The price publication is too old to plan against.
    #[error("price book is stale: {age_s}s old, limit {limit_s}s")]
    StalePrices {
        /// Age at plan time.
        age_s: i64,
        /// Allowed age.
        limit_s: i64,
    },
    /// The price book does not cover enough of the horizon to plan anything.
    #[error("prices cover {slots} slot(s); at least {minimum} are needed")]
    PriceCoverage {
        /// Slots the book covers from now.
        slots: usize,
        /// Minimum the planner needs.
        minimum: usize,
    },
    /// A measured statistics series has a hole; it is never zero-filled.
    #[error("statistics {series} have a gap at {t_start}")]
    StatsGap {
        /// Which series.
        series: &'static str,
        /// First missing slot start.
        t_start: i64,
    },
    /// A statistics series the plan needs is not configured.
    #[error("statistics {series} are not configured")]
    StatsMissing {
        /// Which series.
        series: &'static str,
    },
    /// The energy dashboard is not configured, so there is no measured base at all.
    #[error("the Home Assistant energy dashboard is not configured")]
    SiteNotConfigured,
    /// Neither a fresh forecast nor enough measured history exists to build one.
    #[error("no forecast for {name}: {reason}")]
    ForecastUnavailable {
        /// Which series.
        name: &'static str,
        /// Why the fallback could not be built.
        reason: String,
    },
    /// The price layer refused a slot or the book.
    #[error(transparent)]
    Price(#[from] PriceError),
    /// The upstream read (Home Assistant, statistics) failed.
    #[error("plan supply read failed: {reason}")]
    Supply {
        /// Human-readable cause.
        reason: String,
    },
    /// The optimizer refused the scenario.
    #[error(transparent)]
    Solve(#[from] SolveError),
    /// The solver returned a different number of slots than the horizon.
    #[error("solver returned {got} slots for a {expected}-slot horizon")]
    SolveShape {
        /// Horizon slots.
        expected: usize,
        /// Returned slots.
        got: usize,
    },
    /// The periodic differential against the brute-force oracle disagreed (`PLAN.md` §6).
    #[error("the oracle differential disagreed: {detail}")]
    Differential {
        /// Which field disagreed, and the two values.
        detail: String,
    },
}
