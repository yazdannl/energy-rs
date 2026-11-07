//! Prices and settlement: the single source of truth for money.
//!
//! Responsibility (PLAN.md §2 and §4, `reports/energy-rs/prices.md`):
//!
//! * [`book`] — validated, gap-free [`PriceBook`] slots with `buy_all_in` exactly as published and
//!   `market` derived once at ingest; exact-or-refuse on gaps, never zero-filled.
//! * [`provider`] — [`PriceProvider`] with `ZonneplanDirect` (refresh-token OAuth2 + consumer-prices
//!   chart) and `HaSensorFallback` (integration sensor forecast, coverage-based staleness).  Prices
//!   are fetched once per publication, never per tick.
//! * [`settlement`] — contract constants (VAT, yearly energy tax, fees, Zonnebonus guards) and the
//!   one [`Settlement::revenue`] / [`Settlement::export_price`] / [`Settlement::buy`] used by the
//!   optimizer, dispatch, the sell floor and reporting.  Net metering is date-driven with a manual
//!   override.
//! * [`store`] — append-only publications in memory or SQLite (WAL, in-code migrations), idempotent
//!   saves by content hash, newest publication per slot on load.
//!
//! Money is EUR/kWh in f64, rounded to 1e-6 on ingest and write; time is UTC epoch seconds, tariff
//! dates and years are `Europe/Amsterdam`.

pub mod book;
pub mod error;
pub mod provider;
pub mod settlement;
pub mod store;

pub use book::{PriceBook, PriceQuery, Slot, SlotSource};
pub use error::PriceError;
pub use provider::{
    BoxFuture, HaCredentials, HaSensorFallback, PriceProvider, ZonneplanCredentials,
    ZonneplanDirect,
};
pub use settlement::{
    BonusRules, Contract, Daylight, EnergyTaxTable, ExportSource, NetMetering, SALDERING_END_YEAR,
    Saldering, Settlement, TaxYear,
};
pub use store::{
    MemoryPriceRepository, PriceRepository, Publication, SaveOutcome, SqlitePriceRepository,
};
