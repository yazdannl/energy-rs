//! One quarter-hour the interval planner may use.

/// A plan slot as the maintenance planner sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeCandidate {
    /// Plan slot's original start, used to find the row when folding the charge back in.
    pub slot_start: i64,
    /// Effective charge start: the plan slot start, or `now` for the current partial slot.
    pub t_start: i64,
    /// Effective charge end, clipped to the completion deadline.
    pub t_end: i64,
    /// Published all-in import price, EUR/kWh.
    pub price_eur_per_kwh: f64,
    /// Grid import headroom left for maintenance charging in this slot, W.
    pub available_w: f64,
}
