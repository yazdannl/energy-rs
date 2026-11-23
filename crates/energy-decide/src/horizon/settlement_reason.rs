//! Typed reasons for a slot's export price; no arbitrary strings cross the boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementReason {
    /// A configured budget still has netting allowance.
    ActiveBudget,
    /// No budget information is available.
    SalderingBudgetUnknown,
    /// Budget confirmed to be zero.
    SalderingBudgetZero,
    /// Budget has been consumed.
    SalderingBudgetExhausted,
    /// Operator acknowledged untracked netting.
    UntrackedAcknowledged,
    /// The slot's tariff date is outside the netting regime.
    NetMeteringInactive,
}

impl SettlementReason {
    /// Stable external slug for logs and plan JSON.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::ActiveBudget => "active_budget",
            Self::SalderingBudgetUnknown => "saldering_budget_unknown",
            Self::SalderingBudgetZero => "saldering_budget_zero",
            Self::SalderingBudgetExhausted => "saldering_budget_exhausted",
            Self::UntrackedAcknowledged => "untracked_acknowledged",
            Self::NetMeteringInactive => "net_metering_inactive",
        }
    }
}
