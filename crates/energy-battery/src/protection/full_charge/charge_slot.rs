//! One chosen quarter-hour of a maintenance charge.

/// A quarter-hour the plan reserves for a due pack's balancing charge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeSlot {
    /// Original plan-slot start, so the fold finds the right row.
    pub slot_start: i64,
    /// Effective charge start.
    pub t_start: i64,
    /// Effective charge end.
    pub t_end: i64,
    /// Actual charging time in this slot, seconds (shorter for the first/last clipped slot).
    pub duration_s: i64,
    /// Import price in this slot, EUR/kWh.
    pub price_eur_per_kwh: f64,
    /// AC charge power reserved for this pack, W.
    pub charge_w: f64,
}
