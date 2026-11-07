//! Contract constants and the one settlement function.  See PLAN.md §4 and
//! `reports/energy-rs/prices.md` §2, §3 and §6.
//!
//! The published all-in price is the only per-slot input; taxes and fees are yearly constants.
//! `market` is derived once at ingest, export follows one formula with the Zonnebonus guards, and
//! the net-metering regime is one date-driven switch with a manual override.

pub mod bonus;
pub mod contract;
pub mod daylight;
pub mod export_source;
pub mod net_metering;
pub mod saldering;
pub mod billing;
pub mod tax_table;
pub mod tax_year;

pub use bonus::BonusRules;
pub use contract::Contract;
pub use daylight::Daylight;
pub use export_source::ExportSource;
pub use net_metering::{NetMetering, SALDERING_END_YEAR};
pub use saldering::Saldering;
pub use billing::Settlement;
pub use tax_table::EnergyTaxTable;
pub use tax_year::TaxYear;
