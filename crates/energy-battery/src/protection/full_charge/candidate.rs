//! One quarter-hour the interval planner may use.

/// A plan slot as the maintenance planner sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeCandidate {
    /// UTC epoch seconds of the slot start.
    pub t_start: i64,
    /// UTC epoch seconds of the slot end.
    pub t_end: i64,
    /// Published all-in import price, EUR/kWh.
    pub price_eur_per_kwh: f64,
    /// Grid import headroom left for maintenance charging in this slot, W.
    pub available_w: f64,
}
