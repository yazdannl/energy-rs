//! The plan the engine is following.

use crate::plan::PlanSummary;

/// One generated plan as the engine holds it (the timeline lives in the ledger).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSnapshot {
    /// Ledger id of the stored plan, so the API can read its slots.
    pub plan_id: i64,
    /// When the plan was solved, UTC epoch seconds.
    pub generated_at: i64,
    /// First slot start, UTC epoch seconds.
    pub horizon_start: i64,
    /// Exclusive end of the horizon, UTC epoch seconds.
    pub horizon_end: i64,
    /// Whether the configured terminal target was reached.
    pub feasible: bool,
    /// Money and energy totals of the planned path.
    pub summary: PlanSummary,
    /// Bound slugs that shaped the plan (`price_change`, `soc_deviation`, …).
    pub binding_reasons: Vec<String>,
}
