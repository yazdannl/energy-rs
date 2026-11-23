//! One required price slot as handed to the horizon assembler.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceSlot {
    /// UTC epoch second of this slot's start.
    pub t_start: i64,
    /// Provider resolution in seconds; the planner requires exactly 900.
    pub slot_seconds: i64,
    /// Published all-in buy price, EUR/kWh.
    pub buy_all_in: f64,
    /// Raw post-saldering feed-in price, EUR/kWh; signed, never clamped here.
    pub feed_in_post: f64,
}
