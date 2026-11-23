//! Saldering budget states; absent budget is unknown, never an unlimited sentinel.

use crate::horizon::SettlementReason;

#[derive(Debug, Clone, PartialEq)]
pub enum BudgetState {
    /// A configured, still-available 1:1 budget.
    Active { remaining_kwh: f64 },
    /// No budget information: conservative feed-in price.
    Unknown,
    /// Confirmed zero budget.
    Zero,
    /// Previously available budget has been consumed.
    Exhausted,
    /// Operator acknowledged untracked netting (1:1 only inside an active regime).
    UntrackedAcknowledged,
}

impl BudgetState {
    /// Typed reason attached to the export price.
    pub const fn reason(&self) -> SettlementReason {
        match self {
            Self::Active { .. } => SettlementReason::ActiveBudget,
            Self::Unknown => SettlementReason::SalderingBudgetUnknown,
            Self::Zero => SettlementReason::SalderingBudgetZero,
            Self::Exhausted => SettlementReason::SalderingBudgetExhausted,
            Self::UntrackedAcknowledged => SettlementReason::UntrackedAcknowledged,
        }
    }

    /// Where the 1:1 entitlement came from, when one applies.
    pub const fn budget_source(&self) -> &'static str {
        match self {
            Self::Active { .. } => "configured",
            Self::Unknown => "unknown",
            Self::Zero => "zero",
            Self::Exhausted => "exhausted",
            Self::UntrackedAcknowledged => "untracked",
        }
    }
}
