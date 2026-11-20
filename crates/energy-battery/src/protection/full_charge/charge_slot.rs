//! One chosen quarter-hour of a maintenance charge.

/// A quarter-hour the plan reserves for a due pack's balancing charge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeSlot {
    /// UTC epoch seconds of the slot start.
    pub t_start: i64,
    /// UTC epoch seconds of the slot end.
    pub t_end: i64,
    /// Import price in this slot, EUR/kWh.
    pub price_eur_per_kwh: f64,
    /// AC charge power reserved for this pack, W.
    pub charge_w: f64,
}
