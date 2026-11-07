//! Worked examples from `reports/energy-rs/prices.md`: the published build-up inverted, the
//! post-saldering feed-in, the two negative-price thresholds, and what one slot's net flow earns.

mod common;

use common::{ENERGY_TAX_EX, FEE_INCL, contract, quarter_hourly, slot_at};
use energy_price::settlement::{ExportSource, NetMetering, Saldering, Settlement};

const BUY: f64 = 0.20;
/// `(0.20 - 0.02) / 1.21 - 0.09161` in full f64 precision.
const MARKET_EXACT_AT_0_20: f64 = 0.05715033057851243;
/// `market + 0.02` -- the researched 0.0772 EUR/kWh to four decimals.
const FEED_IN_AT_0_20: f64 = 0.07715;

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-9
}

/// 2026 (saldering on) with a fresh annual budget of `import_kwh`.
fn saldering_2026(import_kwh: f64) -> Settlement {
    let mut settlement = Settlement::on(contract(), common::date("2026-11-20"));
    settlement.set_import_ytd(2026, import_kwh);
    settlement
}

/// 2027 (saldering off by date) with no annual netting.
fn post_saldering_2027() -> Settlement {
    Settlement::on(contract(), common::date("2027-06-01"))
}

#[test]
fn market_is_the_published_build_up_inverted() {
    let contract = contract();
    let market = contract
        .market_from_buy(BUY, 2026)
        .expect("2026 is configured");
    assert!(close(market, MARKET_EXACT_AT_0_20), "market was {market}");
    // Zonneplan's own example: (0.0916 + 0.057) * 1.21 + 0.02 = 0.20.
    assert!(close(
        contract.all_in(0.057, 2026).expect("2026"),
        0.1998181
    ));
    // The inverse and the build-up are the same function.
    assert!(close(contract.all_in(market, 2026).expect("2026"), BUY));
    assert!(close(contract.fee_ex(), FEE_INCL / (1.0 + contract.vat)));
}

#[test]
fn the_feed_in_is_market_plus_the_supplier_fee() {
    let settlement = post_saldering_2027();
    let slot = slot_at("2027-06-01T13:00:00+02:00", BUY);
    // Battery export earns no Zonnebonus: (0.20 - 0.0200) / 1.21 - 0.09161 + 0.0200 = 0.07715,
    // which reproduces the published 7.92 ct average to four decimals.
    let feed_in = settlement.feed_in_post(&slot, ExportSource::Battery);
    assert!(close(feed_in, FEED_IN_AT_0_20), "feed-in was {feed_in}");
    assert!((feed_in - 0.0772).abs() < 1e-4);
    // No return costs for Zonneplan, so the export price is the feed-in.
    assert!(close(
        settlement.export_price(&slot, ExportSource::Battery),
        feed_in
    ));
    // Daylight PV export adds the 10 % Zonnebonus on top, never the battery's.
    assert!(close(
        settlement.feed_in_post(&slot, ExportSource::Pv),
        feed_in * 1.1
    ));
}

#[test]
fn the_all_in_price_turns_negative_at_the_documented_market_price() {
    let contract = contract();
    // prices.md §4: all-in < 0 when market < -0.10814, feed-in < 0 when market < -0.02.
    let break_even = contract
        .market_from_buy(0.0, 2026)
        .expect("zero all-in is representable");
    assert!(close(break_even, -0.1081389256198347));
    assert!(close(
        contract
            .all_in(-(ENERGY_TAX_EX + contract.fee_ex()), 2026)
            .expect("2026"),
        0.0
    ));
    // feed_in = market + fee, so it hits zero at market = -fee, i.e. all-in 0.1066481.
    let zero_feed_in_buy = contract.all_in(-FEE_INCL, 2026).expect("2026");
    assert!(close(zero_feed_in_buy, 0.1066481));
    assert!(close(
        post_saldering_2027().feed_in_post(
            &slot_at("2027-06-01T13:00:00+02:00", zero_feed_in_buy),
            ExportSource::Pv
        ),
        0.0
    ));
}

#[test]
fn a_negative_export_price_is_refused_not_charged() {
    let settlement = post_saldering_2027();
    // market = -0.0255, feed-in = -0.0055: exporting would cost money, so it is worth nothing.
    let slot = slot_at("2027-06-01T13:00:00+02:00", 0.10);
    assert!(settlement.feed_in_post(&slot, ExportSource::Pv) < 0.0);
    assert_eq!(settlement.export_price(&slot, ExportSource::Pv), 0.0);
    assert_eq!(settlement.revenue(&slot, 3.0, ExportSource::Battery), 0.0);
    assert_eq!(
        settlement.marginal_export(&slot, ExportSource::Battery),
        0.0
    );
}

#[test]
fn a_negative_import_price_pays_you_and_still_blocks_export() {
    // market = -0.1495: importing pays 5 ct per kWh, exporting costs money even while netting.
    let settlement = saldering_2026(0.0);
    let slot = slot_at("2026-11-20T12:00:00+01:00", -0.05);
    assert!(close(settlement.buy(&slot), -0.05));
    assert!(close(
        settlement.revenue(&slot, -2.0, ExportSource::Other),
        0.10
    ));
    // Export "un-nets" at a negative all-in price, so the price is clamped to zero.
    assert_eq!(settlement.export_price(&slot, ExportSource::Pv), 0.0);
    assert_eq!(settlement.revenue(&slot, 2.0, ExportSource::Pv), 0.0);
}

#[test]
fn a_netted_export_earns_the_whole_all_in_price() {
    let mut settlement = saldering_2026(10.0);
    let slot = slot_at("2026-11-20T12:00:00+01:00", BUY);
    assert!(settlement.saldering_active(&slot));
    assert!(close(settlement.budget_remaining(&slot), 10.0));
    assert!(close(settlement.export_price(&slot, ExportSource::Pv), BUY));
    assert!(close(
        settlement.revenue(&slot, 4.0, ExportSource::Pv),
        0.80
    ));
    // Settling advances the budget by the same arithmetic the revenue used.
    assert!(close(settlement.settle(&slot, 8.0, ExportSource::Pv), 1.60));
    assert!(close(settlement.budget_remaining(&slot), 2.0));
}

#[test]
fn the_export_is_netted_first_and_paid_at_feed_in_after_the_budget() {
    let mut settlement = saldering_2026(2.0);
    let slot = slot_at("2026-11-20T12:00:00+01:00", BUY);
    settlement.settle(&slot, 2.0, ExportSource::Pv);
    assert!(close(settlement.budget_remaining(&slot), 0.0));
    // Battery export after the budget: the full feed-in price, never the all-in price.
    let revenue = settlement.revenue(&slot, 3.0, ExportSource::Battery);
    assert!(close(revenue, 3.0 * FEED_IN_AT_0_20), "got {revenue}");
    assert!(close(
        settlement.export_price(&slot, ExportSource::Battery),
        FEED_IN_AT_0_20
    ));
}

#[test]
fn the_marginal_export_value_is_the_displacement_of_later_pv() {
    let mut settlement = saldering_2026(100.0);
    let slot = slot_at("2026-11-20T12:00:00+01:00", BUY);
    // prices.md §3.2: buy - feed_in_post ~ 0.20 - 0.078 = 0.12, not the 0.20 all-in price.
    let marginal = settlement.marginal_export(&slot, ExportSource::Battery);
    assert!(close(marginal, 0.12285), "got {marginal}");
    assert!(marginal < settlement.buy(&slot));
    // Self-consumption replaces an import and is worth the whole all-in price.
    assert!(close(settlement.buy(&slot), BUY));

    // Once the budget is gone the marginal value is the plain feed-in price.
    settlement.set_budget(Some(Saldering::new(1.0, 1.0, 2026)));
    assert!(close(
        settlement.marginal_export(&slot, ExportSource::Battery),
        FEED_IN_AT_0_20
    ));
}

#[test]
fn the_regime_is_date_driven_with_a_manual_override() {
    let contract = contract();
    let on_2026 = slot_at("2026-12-31T23:45:00+01:00", BUY);
    let on_2027 = slot_at("2027-01-01T00:00:00+01:00", BUY);

    let mut settlement = saldering_2026(100.0);
    assert!(settlement.saldering_active(&on_2026));
    assert!(!settlement.saldering_active(&on_2027));
    // 2027 has no budget, so export is the post-saldering price.
    assert!(close(
        settlement.export_price(&on_2027, ExportSource::Pv),
        FEED_IN_AT_0_20
    ));

    // A legacy contract can force netting on; forcing it off in 2026 flips the price.
    settlement.set_regime(NetMetering::On);
    settlement.set_budget(Some(Saldering::new(100.0, 0.0, 2027)));
    assert!(settlement.saldering_active(&on_2027));

    let mut off = saldering_2026(100.0);
    off.set_regime(NetMetering::Off);
    assert!(!off.saldering_active(&on_2026));
    assert!(close(
        off.export_price(&on_2026, ExportSource::Pv),
        FEED_IN_AT_0_20
    ));
    assert!(close(
        off.marginal_export(&on_2026, ExportSource::Pv),
        FEED_IN_AT_0_20
    ));

    let mut date_driven = Settlement::on(contract, common::date("2026-11-20"));
    assert_eq!(date_driven.today(), common::date("2026-11-20"));
    date_driven.set_today(common::date("2027-01-02"));
    assert_eq!(date_driven.today(), common::date("2027-01-02"));
    assert_eq!(date_driven.regime(), NetMetering::DateDriven);
}

#[test]
fn a_book_of_slots_prices_each_slot_independently() {
    let settlement = post_saldering_2027();
    let book = common::book(quarter_hourly(
        "2027-06-01T02:00:00+02:00",
        &[0.10, 0.20, -0.05],
    ));
    let prices: Vec<f64> = book
        .iter()
        .map(|slot| settlement.export_price(slot, ExportSource::Pv))
        .collect();
    assert_eq!(prices.len(), 3);
    // Negative feed-in (0.10) refuses export, 0.20 pays the feed-in, negative all-in pays nothing.
    assert_eq!(prices[0], 0.0);
    assert!(close(prices[1], FEED_IN_AT_0_20));
    assert_eq!(prices[2], 0.0);
}
