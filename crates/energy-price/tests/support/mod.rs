//! Shared fixtures for the integration tests: the researched Zonneplan contract, a slot builder
//! whose `market` is derived the way providers derive it, and date/instant helpers.
#![allow(dead_code)]
// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here, and fixture timestamps/money read better verbatim than with digit separators.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::unreadable_literal
)]

use chrono::{DateTime, NaiveDate};
use energy_price::book::query::{local_date_of, local_year_of};
use energy_price::{Contract, EnergyTaxTable, PriceBook, Slot, SlotSource, TaxYear};

/// Zonneplan inkoopvergoeding, EUR/kWh incl. VAT (prices.md §7 row 5).
pub const FEE_INCL: f64 = 0.0200;
/// Energiebelasting 2026, EUR/kWh excl. VAT (prices.md §7 row 1).
pub const ENERGY_TAX_EX: f64 = 0.09161;

/// 2026 constants from prices.md §7.  The 2027 rate was not definitive when the report was written,
/// so the tests hold it equal: every worked example stays checkable against the 2026 formula.
pub fn contract() -> Contract {
    let tax = EnergyTaxTable::new(vec![
        TaxYear::new(2026, TaxYear::ENERGY_TAX_2026),
        TaxYear::new(2027, TaxYear::ENERGY_TAX_2026),
    ])
    .expect("the 2026/2027 tax table is valid");
    Contract::zonneplan_2026().with_tax(tax)
}

/// Epoch seconds of an RFC 3339 instant.
pub fn instant(rfc3339: &str) -> i64 {
    DateTime::parse_from_rfc3339(rfc3339)
        .expect("the fixture timestamp is RFC 3339")
        .timestamp()
}

/// A quarter-hourly slot at `rfc3339`, with `market` derived exactly as ingest derives it.
pub fn slot_at(rfc3339: &str, buy_all_in: f64) -> Slot {
    market_slot(rfc3339, buy_all_in, SlotSource::ZonneplanQuarterHourly)
}

/// A slot of the given resolution at `rfc3339`, with `market` derived as ingest derives it.
pub fn market_slot(rfc3339: &str, buy_all_in: f64, source: SlotSource) -> Slot {
    slot_from_instant(instant(rfc3339), buy_all_in, source)
}

/// A slot at an epoch instant, with `market` derived as ingest derives it.
pub fn slot_from_instant(t_start: i64, buy_all_in: f64, source: SlotSource) -> Slot {
    let year = local_year_of(t_start).expect("the fixture instant has a local year");
    let market = contract()
        .market_from_buy(buy_all_in, year)
        .expect("the year is configured");
    Slot::new(t_start, buy_all_in, market, source).money_rounded()
}

/// Consecutive quarter-hourly slots starting at `start`, one all-in price each.
pub fn quarter_hourly(start: &str, buy_all_in: &[f64]) -> Vec<Slot> {
    let first = instant(start);
    buy_all_in
        .iter()
        .enumerate()
        .map(|(index, buy)| {
            let offset = i64::try_from(index).expect("a small fixture index") * 900;
            slot_from_instant(first + offset, *buy, SlotSource::ZonneplanQuarterHourly)
        })
        .collect()
}

/// A validated book from slots (panics on a hole: fixtures must be contiguous).
pub fn book(slots: Vec<Slot>) -> PriceBook {
    PriceBook::new(slots).expect("the fixture slots form a gap-free book")
}

/// Local tariff date of an instant.
pub fn date_of(rfc3339: &str) -> NaiveDate {
    local_date_of(instant(rfc3339)).expect("the fixture instant has a local date")
}

/// A local date literal.
pub fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("the fixture date is YYYY-MM-DD")
}
