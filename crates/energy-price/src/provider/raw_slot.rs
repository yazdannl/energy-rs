//! One parsed price before validation.

/// One parsed price before validation.
#[derive(Debug, Clone, Copy)]
pub struct RawSlot {
    /// Slot start instant, UTC epoch seconds.
    pub t_start: i64,
    /// EUR/kWh exactly as published (fixed point already divided by 1e7).
    pub buy_all_in: f64,
    /// EUR/kWh as published without the energy tax, when the provider publishes it. This is the
    /// feed-in price after net metering ends, so it is carried verbatim instead of re-derived.
    pub buy_ex_tax: Option<f64>,
}
