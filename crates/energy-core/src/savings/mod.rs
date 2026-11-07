//! Savings accounting: what the battery actually saved, per measured local day.
//!
//! The method is the counterfactual, not the plan: every slot of a day is measured from the energy
//! dashboard, settled with [`energy_price::Settlement`] (saldering budget included), and compared
//! with the same PV, the same load and the same prices in a house **without** a battery. The day's
//! saving is `baseline - actual`, split into four parts that add back up to it exactly:
//!
//! * [`DayAccounting::arbitrage_eur`] — battery shifting against the day's flat reference price;
//! * [`DayAccounting::self_consumption_eur`] — PV kept on site instead of exported, valued at the
//!   import price of the slot whose export it replaced;
//! * [`DayAccounting::feed_in_eur`] — what the exported kWh actually earned, actual versus baseline
//!   (the term that carries saldering and negative prices);
//! * [`DayAccounting::battery_balance_eur`] — the battery's own energy balance at the reference
//!   price (round-trip loss, and a pack that ends the day fuller or emptier than it started).
//!
//! The arithmetic and the approximations are in `docs/SAVINGS.md`; [`SavingsRollup`] writes the
//! rows, [`HaSavingsSource`] reads the measurements, and a closed day whose slots were not all
//! measured is stored with `NULL` money and its coverage instead of a fake number.

mod accountant;
mod annual_seeds;
mod day_accounting;
mod day_reading;
mod error;
mod flow_totals;
mod ha_source;
mod measured_day;
mod rollup;
mod rollup_report;
mod settlement_seed;
mod slot_flow;
mod source;
mod statistic_series;

pub use accountant::account;
pub use day_accounting::DayAccounting;
pub use day_reading::DayReading;
pub use error::SavingsError;
pub use flow_totals::FlowTotals;
pub use ha_source::HaSavingsSource;
pub use measured_day::MeasuredDay;
pub use rollup::{CATCH_UP_DAYS, SavingsRollup};
pub use rollup_report::RollupReport;
pub use settlement_seed::SettlementSeed;
pub use slot_flow::SlotFlow;
pub use source::SavingsSource;
