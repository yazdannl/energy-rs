//! The annual saldering budget and the Zonnebonus counters: what moves them, what resets them and
//! which exports are refused a bonus (prices.md §3.2, §3.3).

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

mod support;

use energy_price::Contract;
use energy_price::settlement::{BonusRules, ExportSource, Saldering, Settlement};
use support::{contract, slot_at};

const BUY: f64 = 0.20;
/// Feed-in without the Zonnebonus for the fixture slot (`market + 0.02`).
const FEED_IN: f64 = 0.07715;

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-9
}

fn settlement(today: &str, import_kwh: f64) -> Settlement {
    let mut settlement = Settlement::on(contract(), support::date(today));
    settlement.set_import_ytd(2026, import_kwh);
    settlement
}

#[test]
fn the_budget_is_per_year_and_never_negative() {
    let budget = Saldering::new(12.0, 4.0, 2026);
    assert!(close(budget.budget_remaining(2026), 8.0));
    assert_eq!(budget.budget_remaining(2027), 0.0);
    // Over-consuming clamps at zero; a negative export cannot create budget.
    let mut budget = budget;
    assert!(close(budget.consume(2026, 50.0), 8.0));
    assert_eq!(budget.budget_remaining(2026), 0.0);
    assert_eq!(budget.consume(2026, -5.0), 0.0);
    assert_eq!(budget.netted_ytd_kwh, 12.0);
}

#[test]
fn netting_is_exhausted_over_a_sequence_of_exports() {
    let mut settlement = settlement("2026-11-20", 5.0);
    let cheap = slot_at("2026-11-20T02:00:00+01:00", 0.15);
    let peak = slot_at("2026-11-20T18:00:00+01:00", 0.30);
    // 3 kWh at the low price, netted 1:1 because the budget lasts.
    assert!(close(
        settlement.settle(&cheap, 3.0, ExportSource::Battery),
        0.45
    ));
    assert!(close(settlement.budget_remaining(&peak), 2.0));
    // 4 kWh at the peak price: 2 netted at 0.30, 2 at the feed-in price.
    let peak_market = settlement.feed_in_post(&peak, ExportSource::Battery);
    let revenue = settlement.settle(&peak, 4.0, ExportSource::Battery);
    assert!(
        close(revenue, 2.0 * 0.30 + 2.0 * peak_market),
        "got {revenue}"
    );
    assert_eq!(settlement.budget_remaining(&peak), 0.0);
    // Nothing left: every further kWh earns the feed-in price only.
    assert!(close(
        settlement.revenue(&peak, 1.0, ExportSource::Battery),
        peak_market
    ));
    assert_eq!(
        settlement.settle(&peak, 1.0, ExportSource::Battery),
        peak_market
    );
    assert_eq!(settlement.budget_remaining(&peak), 0.0);
}

#[test]
fn an_import_reading_starts_a_new_year_and_a_new_budget() {
    let mut settlement = settlement("2026-11-20", 500.0);
    settlement.settle(
        &slot_at("2026-11-20T12:00:00+01:00", BUY),
        100.0,
        ExportSource::Pv,
    );
    assert!(close(
        settlement.budget_remaining(&slot_at("2026-11-20T12:00:00+01:00", BUY)),
        400.0
    ));
    // A meter reading for the next year resets both counters before recording the import.
    settlement.set_import_ytd(2027, 40.0);
    let budget = settlement.budget().expect("a budget was configured");
    assert_eq!(budget.year, 2027);
    assert!(close(budget.import_ytd_kwh, 40.0));
    assert_eq!(budget.netted_ytd_kwh, 0.0);
    assert_eq!(budget.budget_remaining(2026), 0.0);
    assert!(close(budget.budget_remaining(2027), 40.0));
}

#[test]
fn without_a_budget_or_import_the_export_is_never_netted() {
    // No budget configured: the 2026 regime cannot credit 1:1, so it pays the feed-in price.
    let settlement = Settlement::on(contract(), support::date("2026-11-20"));
    let slot = slot_at("2026-11-20T12:00:00+01:00", BUY);
    assert!(!settlement.saldering_active(&slot));
    assert_eq!(settlement.budget_remaining(&slot), 0.0);
    // Battery export: the plain feed-in price, and settlement moves no counters.
    assert!(close(
        settlement.export_price(&slot, ExportSource::Battery),
        FEED_IN
    ));
    assert!(close(
        settlement.revenue(&slot, 2.0, ExportSource::Other),
        2.0 * FEED_IN
    ));
}

#[test]
fn a_non_positive_market_price_earns_no_bonus() {
    // market = -0.0255: `market + fee` is negative, so the daylight PV guard refuses the bonus.
    let settlement = Settlement::on(contract(), support::date("2027-06-01"));
    let slot = slot_at("2027-06-01T13:00:00+02:00", 0.10);
    assert!(settlement.feed_in_post(&slot, ExportSource::Pv) < 0.0);
    assert_eq!(settlement.export_price(&slot, ExportSource::Pv), 0.0);
}

#[test]
fn the_bonus_needs_daylight_pv_and_budget_left_under_the_cap() {
    let mut settlement = Settlement::on(contract(), support::date("2027-06-01"));
    let midday = slot_at("2027-06-01T13:00:00+02:00", BUY);
    let night = slot_at("2027-06-01T03:00:00+02:00", BUY);
    let feed_in = settlement.feed_in_post(&midday, ExportSource::Battery);
    assert!(close(feed_in, FEED_IN));

    // Daylight PV export: 10 % on top.  Night PV and battery export: plain feed-in.
    assert!(close(
        settlement.export_price(&midday, ExportSource::Pv),
        feed_in * 1.1
    ));
    assert!(close(
        settlement.export_price(&night, ExportSource::Pv),
        feed_in
    ));
    assert!(close(
        settlement.export_price(&midday, ExportSource::Battery),
        feed_in
    ));

    // The cap is 7 500 kWh of bonus-earning export: at the cap the multiplier disappears.
    settlement.set_bonus_ytd_kwh(2027, 7_500.0);
    assert!(close(
        settlement.export_price(&midday, ExportSource::Pv),
        feed_in
    ));
    settlement.set_bonus_ytd_kwh(2027, 7_499.0);
    assert!(close(
        settlement.export_price(&midday, ExportSource::Pv),
        feed_in * 1.1
    ));
    // A counter from another year does not suppress this year's bonus.
    settlement.set_bonus_ytd_kwh(2026, 7_500.0);
    assert!(close(
        settlement.export_price(&midday, ExportSource::Pv),
        feed_in * 1.1
    ));
}

#[test]
fn settlement_advances_the_bonus_counter_by_the_un_netted_export() {
    let mut settlement = Settlement::on(contract(), support::date("2026-11-20"));
    settlement.set_import_ytd(2026, 2.0);
    let slot = slot_at("2026-11-20T12:00:00+01:00", BUY);
    settlement.settle(&slot, 5.0, ExportSource::Pv);
    // 2 kWh were netted 1:1 (no bonus), 3 kWh earned the feed-in price and count against the cap.
    assert!(
        close(settlement.bonus_ytd_kwh(), 3.0),
        "got {}",
        settlement.bonus_ytd_kwh()
    );

    // A restored counter is taken as given, and a broken one is refused.
    settlement.set_bonus_ytd_kwh(2026, 10.0);
    assert_eq!(settlement.bonus_ytd_kwh(), 10.0);
    assert_eq!(settlement.bonus_year(), 2026);
    settlement.set_bonus_ytd_kwh(2026, f64::NAN);
    assert_eq!(settlement.bonus_ytd_kwh(), 0.0);
    settlement.set_bonus_ytd_kwh(2026, -4.0);
    assert_eq!(settlement.bonus_ytd_kwh(), 0.0);
}

#[test]
fn the_bonus_counter_starts_over_in_a_new_year() {
    let mut settlement = Settlement::on(contract(), support::date("2026-12-31"));
    settlement.set_bonus_ytd_kwh(2026, 7_000.0);
    // Settling a 2027 slot moves the annual counter forward instead of carrying 7 000 kWh into it.
    let new_year = slot_at("2027-01-05T13:00:00+01:00", BUY);
    settlement.settle(&new_year, 2.0, ExportSource::Pv);
    assert_eq!(settlement.bonus_year(), 2027);
    assert!(
        close(settlement.bonus_ytd_kwh(), 2.0),
        "got {}",
        settlement.bonus_ytd_kwh()
    );
    // Replaying an older slot never rolls the counter back.
    let old = slot_at("2026-12-31T13:00:00+01:00", BUY);
    settlement.settle(&old, 1.0, ExportSource::Pv);
    assert_eq!(settlement.bonus_year(), 2027);
    assert!(
        close(settlement.bonus_ytd_kwh(), 3.0),
        "got {}",
        settlement.bonus_ytd_kwh()
    );
}

#[test]
fn a_contract_without_a_bonus_never_multiplies_the_feed_in() {
    let mut contract: Contract = contract();
    contract.bonus = BonusRules::none();
    let settlement = Settlement::on(contract, support::date("2027-06-01"));
    let midday = slot_at("2027-06-01T13:00:00+02:00", BUY);
    assert!(close(
        settlement.export_price(&midday, ExportSource::Pv),
        FEED_IN
    ));
}

#[test]
fn contract_constants_are_versioned_by_year_and_refuse_an_unknown_year() {
    let contract = contract();
    assert!(contract.validate().is_ok());
    assert!(contract.tax_year(2026).is_ok());
    // 2028 is not configured: the market price is refused, never derived with 2027's rate.
    assert!(contract.market_from_buy(BUY, 2028).is_err());
    assert!(contract.all_in(0.05, 2028).is_err());
}
