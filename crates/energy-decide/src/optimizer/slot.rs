//! One optimizer slot: the measured forecast and the prices that settle it.

/// A priced slot with forecast net load and its per-slot site export ceiling.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    /// Slot start, UTC epoch seconds (evidence only; M2 never reads a clock).
    pub t_start: i64,
    /// Unelapsed seconds of this slot. All energy terms use this, never a hardcoded 900 s.
    pub dt_s: i64,
    /// All-in import price, EUR/kWh.
    pub buy_eur_per_kwh: f64,
    /// Export price after settlement, EUR/kWh.
    pub sell_eur_per_kwh: f64,
    /// Forecast house load, kW (non-battery).
    pub load_kw: f64,
    /// Forecast available PV before curtailment, kW.
    pub pv_kw: f64,
    /// Configured PV preference price; `None` means `Auto` (equal to `sell`).
    pub pv_price_eur_per_kwh: Option<f64>,
    /// Export ceiling, kW; `None` means unlimited, `Some(0.0)` means zero export.
    pub export_limit_kw: Option<f64>,
}

impl Slot {
    /// Unelapsed duration in hours.
    pub fn dt_h(&self) -> f64 {
        self.dt_s as f64 / 3600.0
    }

    /// `Auto` preference prices PV at the slot's export price.
    pub fn pv_price(&self) -> f64 {
        self.pv_price_eur_per_kwh.unwrap_or(self.sell_eur_per_kwh)
    }
}
