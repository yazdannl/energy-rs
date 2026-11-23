//! Every objective term with its unit, kept distinct for financial assertions.

/// The cost of one optimized path, split by term.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveBreakdown {
    /// Buy minus sell settlement, EUR; the curtailment loss is already priced here.
    pub grid_eur: f64,
    /// Wear on stored kWh discharged, EUR.
    pub wear_eur: f64,
    /// Policy spread floor, EUR-equivalent preference; never settled money or in `total_eur`.
    pub policy_min_spread_eur: f64,
    /// Negative terminal salvage, EUR.
    pub salvage_eur: f64,
    /// One-sided terminal shortfall penalty, EUR.
    pub terminal_penalty_eur: f64,
    /// Spec economic cost (grid + wear + salvage + terminal penalty), minimised by the DP.
    pub total_eur: f64,
    /// Policy preference, EUR; reported separately, never part of `total_eur`.
    pub policy_preference_eur: f64,
}
