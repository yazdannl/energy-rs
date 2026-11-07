//! One parsed price before validation.

/// One parsed price before validation.
#[derive(Debug, Clone, Copy)]
pub struct RawSlot {
    /// Slot start instant, UTC epoch seconds.
    pub t_start: i64,
    /// EUR/kWh exactly as published (fixed point already divided by 1e7).
    pub buy_all_in: f64,
}
