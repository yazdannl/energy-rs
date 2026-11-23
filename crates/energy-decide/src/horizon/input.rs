//! Everything M1 needs to decide coverage without touching I/O.

use crate::horizon::{BudgetState, GridSource, OperatorOverride, PriceSlot};

#[derive(Debug, Clone, PartialEq)]
pub struct HorizonInput {
    /// Plan instant, UTC epoch seconds.
    pub now: i64,
    /// Slots requested from the containing slot; 120 in normal operation.
    pub requested_slots: usize,
    /// Price rows supplied by the provider, in any order.
    pub slots: Vec<PriceSlot>,
    /// Publication instant of the price book, UTC epoch seconds.
    pub published_at: i64,
    /// Maximum accepted publication age in seconds (shipped: 21600).
    pub price_max_age_s: i64,
    /// Whether a fresh net-load forecast is available for the covered slots.
    pub forecast_available: bool,
    /// Configured grid, PV and optional counter histories.
    pub grid_sources: Vec<GridSource>,
    /// Stored net-metering value; an unknown value is a refusal, never on.
    pub net_metering: String,
    /// Complete operator contract when an explicit regime override is stored.
    pub operator_override: Option<OperatorOverride>,
    /// Saldering budget state for this plan.
    pub budget: BudgetState,
}
