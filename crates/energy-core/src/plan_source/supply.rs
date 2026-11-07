//! Everything one plan needs, as the supply side hands it over.

use energy_price::{PriceBook, Settlement};

use crate::plan_source::{Forecast, MeasuredHistory};

/// One read of the plan's upstream inputs.
///
/// `price_fetched_at` is the publication age, not the plan instant: a fresh fetch of an old book is
/// still an old book, and the source refuses it. `forecast` is `None` when no provider produced one;
/// the source then builds the EWMA fallback from `history` and marks the plan degraded.
#[derive(Debug, Clone, PartialEq)]
pub struct Supply {
    /// Validated measured history, contiguous quarter-hours.
    pub history: MeasuredHistory,
    /// The forward forecast, when a provider produced one.
    pub forecast: Option<Forecast>,
    /// The validated price book (may end early: tomorrow is not published before ~13:00 CET).
    pub prices: PriceBook,
    /// When the price publication was fetched, UTC epoch seconds.
    pub price_fetched_at: i64,
    /// The settlement in force (contract, regime, saldo budget, counters).
    pub settlement: Settlement,
}
