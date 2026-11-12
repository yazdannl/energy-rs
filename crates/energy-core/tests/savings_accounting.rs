//! Savings accounting: the counterfactual, the four-way split and the completeness rule.
//!
//! Every day is synthetic but physically consistent: the measured import/export is derived from
//! `load + charge - pv - discharge`, so the counterfactual identity holds by construction and a
//! wrong split shows up as a mismatched sum rather than a plausible-looking number.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::savings::{SlotFlow, account};
use support::savings::{self, assert_row_split, assert_split};

/// A house with PV but no battery saves exactly nothing: actual and baseline are the same day.
#[test]
fn a_day_without_a_battery_saves_nothing() {
    let day = "2027-06-15";
    let reading = savings::reading(
        day,
        |_| 0.20,
        |_| 0.05,
        |index| {
            let pv = if (40..50).contains(&index) { 0.3 } else { 0.0 };
            (pv, 0.1, 0.0, 0.0)
        },
        savings::unseeded(),
    );
    let accounting = account(&reading, &savings::settlement(day));
    assert!(accounting.complete);
    assert!((accounting.saving_eur).abs() < 1e-12);
    assert!((accounting.actual_eur - accounting.baseline_eur).abs() < 1e-12);
    assert_split(&accounting);
    assert_row_split(&accounting.row(true));
}

/// Battery arbitrage: charge at a cheap slot, discharge at an expensive one, no PV at all.
#[test]
fn a_cheap_charge_expensively_discharged_is_arbitrage() {
    let day = "2027-06-15";
    let reading = savings::reading(
        day,
        |index| match index {
            2 => 0.10,
            60 => 0.30,
            _ => 0.20,
        },
        |_| 0.05,
        |index| match index {
            2 => (0.0, 0.3, 0.2, 0.0),
            60 => (0.0, 0.3, 0.0, 0.2),
            _ => (0.0, 0.3, 0.0, 0.0),
        },
        savings::unseeded(),
    );
    let accounting = account(&reading, &savings::settlement(day));
    assert!(accounting.complete);
    assert!(
        (accounting.saving_eur - 0.04).abs() < 1e-12,
        "{accounting:?}"
    );
    assert!((accounting.arbitrage_eur - 0.04).abs() < 1e-12);
    assert!(accounting.self_consumption_eur.abs() < 1e-12);
    assert!(accounting.feed_in_eur.abs() < 1e-12);
    assert!(accounting.battery_balance_eur.abs() < 1e-12);
    assert_split(&accounting);
}

/// PV stored at noon and used in the evening: worth the avoided import minus the lost feed-in.
#[test]
fn stored_pv_counts_as_self_consumption_and_is_credited_to_the_lost_export() {
    let day = "2027-06-15";
    let reading = savings::reading(
        day,
        |_| 0.30,
        |_| 0.03,
        |index| {
            if (40..50).contains(&index) {
                (0.3, 0.0, 0.3, 0.0)
            } else if (70..80).contains(&index) {
                (0.0, 0.3, 0.0, 0.3)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            }
        },
        savings::unseeded(),
    );
    let accounting = account(&reading, &savings::settlement(day));
    // 3 kWh exported at the post-saldering feed-in: 0.03 market + 2/1.21 ct fee, ex-VAT.
    let lost_export = 3.0 * (0.03 + 0.02 / 1.21);
    assert!(
        (accounting.saving_eur - (0.90 - lost_export)).abs() < 1e-9,
        "{accounting:?}"
    );
    assert!((accounting.self_consumption_eur - 0.90).abs() < 1e-9);
    assert!((accounting.feed_in_eur + lost_export).abs() < 1e-9);
    assert!((accounting.arbitrage_eur).abs() < 1e-12);
    assert!((accounting.pv_kwh - 3.0).abs() < 1e-12);
    assert!(accounting.import_kwh.abs() < 1e-12);
    assert!(accounting.export_kwh.abs() < 1e-12);
    assert_eq!(
        accounting.battery_charge_kwh,
        accounting.battery_discharge_kwh
    );
    assert_split(&accounting);
}

/// Negative midday prices are refused (clamped to zero): storing that PV is worth the evening import.
#[test]
fn an_export_at_a_negative_price_makes_storage_worth_the_full_import_price() {
    let day = "2027-06-15";
    let reading = savings::reading(
        day,
        |_| 0.20,
        |index| {
            if (40..50).contains(&index) {
                -0.10
            } else {
                0.05
            }
        },
        |index| {
            if (40..50).contains(&index) {
                (0.1, 0.0, 0.1, 0.0)
            } else if (70..80).contains(&index) {
                (0.0, 0.1, 0.0, 0.1)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            }
        },
        savings::unseeded(),
    );
    let accounting = account(&reading, &savings::settlement(day));
    assert!(
        (accounting.baseline_eur - 0.20).abs() < 1e-9,
        "{accounting:?}"
    );
    assert!((accounting.actual_eur).abs() < 1e-12);
    assert!((accounting.saving_eur - 0.20).abs() < 1e-9);
    assert!((accounting.self_consumption_eur - 0.20).abs() < 1e-9);
    assert_split(&accounting);
}

/// The saldering budget a day consumes is visible on the next day: day 1 nets 2 kWh 1:1 and leaves
/// nothing, so day 2's export earns only the post-saldering feed-in.
#[test]
fn a_day_consumes_the_saldering_budget_the_next_day_needs() {
    let day = "2026-06-15";
    let sunny = |index: usize| if (30..70).contains(&index) { 0.1 } else { 0.0 };
    let first = savings::reading(
        day,
        |_| 0.30,
        |_| 0.05,
        |index| (sunny(index), 0.0, 0.0, 0.0),
        savings::seeded(2026, 2.0, 0.0),
    );
    let first = account(&first, &savings::settlement(day));
    // 4 kWh exported: 2 kWh netted 1:1 at 0.30 and 2 kWh at the feed-in (0.05 + 2/1.21 ct).
    let feed_in = 2.0 * (0.05 + 0.02 / 1.21);
    assert!(
        (first.baseline_eur + (0.60 + feed_in)).abs() < 1e-9,
        "{first:?}"
    );
    assert!((first.saving_eur).abs() < 1e-12);
    assert_split(&first);

    let tomorrow = "2026-06-16";
    let plan = |index: usize| {
        (
            if (30..50).contains(&index) { 0.1 } else { 0.0 },
            0.0,
            0.0,
            0.0,
        )
    };
    let spent = savings::reading(
        tomorrow,
        |_| 0.30,
        |_| 0.05,
        plan,
        savings::seeded(2026, 2.0, 2.0),
    );
    let spent = account(&spent, &savings::settlement(tomorrow));
    let unspent = savings::reading(
        tomorrow,
        |_| 0.30,
        |_| 0.05,
        plan,
        savings::seeded(2026, 4.0, 0.0),
    );
    let unspent = account(&unspent, &savings::settlement(tomorrow));

    assert!((spent.actual_eur + feed_in).abs() < 1e-9, "{spent:?}");
    assert!((unspent.actual_eur + 0.60).abs() < 1e-9, "{unspent:?}");
    assert!(unspent.actual_eur < spent.actual_eur);
}

/// A closed day with one missing slot still stores its measured saving, marked partial: a single
/// hole used to null the whole day, which is why the page only ever had today's number.
#[test]
fn a_day_with_a_hole_stores_its_measured_saving_as_partial() {
    let day = "2027-06-15";
    let mut reading = savings::reading(
        day,
        |_| 0.20,
        |_| 0.05,
        |_| (0.2, 0.1, 0.0, 0.0),
        savings::unseeded(),
    );
    reading.measured.slots[10].pv_kwh = None;
    let accounting = account(&reading, &savings::settlement(day));
    assert!(!accounting.complete);
    assert!((accounting.coverage_hours - 23.75).abs() < 1e-12);
    let closed = accounting.row(true);
    assert!(
        closed.saving_eur.is_some(),
        "a mostly-covered day is settled"
    );
    assert!(!closed.complete, "and still says it is partial");
    assert!((closed.coverage_hours - 23.75).abs() < 1e-12);
    // The current day is provisional: it keeps the measured-so-far money.
    let provisional = accounting.row(false);
    assert!(provisional.saving_eur.is_some());
    assert!(!provisional.complete);
}

/// A missing price cannot be settled: the day is incomplete even when every flow was measured.
#[test]
fn a_missing_price_keeps_the_energy_but_drops_the_money() {
    let day = "2027-06-15";
    let mut reading = savings::reading(
        day,
        |_| 0.20,
        |_| 0.05,
        |_| (0.2, 0.1, 0.0, 0.0),
        savings::unseeded(),
    );
    let hole = reading.measured.slots[5].t_start;
    reading.prices.remove(&hole);
    let accounting = account(&reading, &savings::settlement(day));
    assert!(!accounting.complete);
    assert!((accounting.coverage_hours - 24.0).abs() < 1e-12);
    assert!(accounting.pv_kwh > 0.0);
    assert!(accounting.row(true).saving_eur.is_none());
}

/// Two meter registers that both moved are one net flow: settlement is on the slot net, so the
/// import and the export are never each priced (prices.md §4.3).
#[test]
fn simultaneous_import_and_export_is_settled_on_the_slot_net() {
    let day = "2027-06-15";
    let mut reading = savings::reading(
        day,
        |_| 0.20,
        |_| 0.05,
        |_| (0.0, 0.0, 0.0, 0.0),
        savings::unseeded(),
    );
    let t_start = reading.measured.slots[20].t_start;
    // 0.2 kWh in and 0.2 kWh out in one slot: a net flow of zero, with a 0.2 kWh house load.
    reading.measured.slots[20] = SlotFlow::measured(t_start, 0.2, 0.2, 0.2, 0.0, 0.0);
    let accounting = account(&reading, &savings::settlement(day));
    assert!(accounting.complete);
    assert!(
        (accounting.actual_eur).abs() < 1e-12,
        "gross pricing would charge 0.04 and credit 0.014: {accounting:?}"
    );
    assert!(accounting.baseline_eur.abs() < 1e-12);
    assert!(accounting.saving_eur.abs() < 1e-12);
    assert_split(&accounting);
}

/// Rounding noise in the balance is measured; a load that is materially negative is a gap.
#[test]
fn an_inconsistent_slot_is_treated_as_a_gap() {
    let mut flow = SlotFlow::measured(0, 0.1, 0.0, 0.0, 0.5, 0.0);
    assert!(!flow.is_measured());
    flow.charge_kwh = Some(0.0);
    assert!(flow.is_measured());
    assert!((flow.load_kwh().unwrap() - 0.1).abs() < 1e-12);
    // 2 Wh of sensor disagreement is noise; 200 Wh is a counter that does not add up.
    let noise = SlotFlow::measured(0, 0.0, 0.002, 0.0, 0.0, 0.0);
    assert!(noise.is_measured());
    let broken = SlotFlow::measured(0, 0.0, 0.2, 0.0, 0.0, 0.0);
    assert!(!broken.is_measured());
}
