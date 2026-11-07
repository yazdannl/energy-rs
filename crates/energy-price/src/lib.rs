//! Prices and settlement.
//!
//! Responsibility (PLAN.md §2, §4): be the single source of truth for money.
//!
//! * [`provider`] — the `PriceProvider` trait with `ZonneplanDirect { refresh_token }` and the
//!   `HaSensors` fallback. Prices are fetched once per publication (detected by max end timestamp
//!   or forecast hash), never per tick.
//! * [`contract`] — the yearly contract constants (`vat`, `energy_tax_ex`, `fee_incl`,
//!   `feed_in_fee`, `bonus_pct`, `teruglever_kosten`, `saldering` regime) and the net-metering
//!   toggle (date driven, manual override).
//! * [`settlement`] — one `revenue(t, kwh, source)` function used by the optimizer, the dispatcher,
//!   the sell floor and reporting. Exact-or-refuse on price gaps.
//! * [`store`] — persisted 15-minute slots plus the import/export budget counters the regime needs.
//!
//! Money is EUR in f64, rounded to 1e-6 on write; energy is Wh in f64.

pub mod contract;
pub mod provider;
pub mod settlement;
pub mod store;
