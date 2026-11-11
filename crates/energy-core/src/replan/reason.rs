//! Why a plan should be replaced.

/// A replan trigger. `None` from [`ReplanTrigger::reason`](crate::replan::ReplanTrigger::reason)
/// means the plan is still valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplanReason {
    /// There is no plan at all.
    NoPlan,
    /// A clock slot boundary was crossed since the plan was generated.
    SlotBoundary,
    /// Live energy deviates from the plan's slot by at least the configured percentage.
    SocDeviation,
    /// A price input changed.
    PriceChanged,
    /// A forecast input changed.
    ForecastChanged,
    /// The plan is older than `max(2 × replan, stale_plan_seconds)`.
    StaleInput,
    /// The horizon no longer covers `now`.
    PlanExpired,
}

impl ReplanReason {
    /// Stable slug for logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoPlan => "no_plan",
            Self::SlotBoundary => "slot_boundary",
            Self::SocDeviation => "soc_deviation",
            Self::PriceChanged => "price_changed",
            Self::ForecastChanged => "forecast_changed",
            Self::StaleInput => "stale_input",
            Self::PlanExpired => "plan_expired",
        }
    }
}
