//! The result of accounting one measured local day.

use energy_ledger::SavingsDaily;

/// Share of the day a closed day must cover before its measured saving is settled.
///
/// A closed day used to store money only at 96/96 coverage, and one missing quarter-hour anywhere in
/// a month of readings left every other day null -- the page showed a single priced day. Missing
/// slots contribute nothing to either side of the comparison, so a mostly-covered day still gives an
/// honest (slightly conservative) saving; it is stored as partial, with its coverage, never as
/// complete.
pub const SETTLE_COVERAGE: f64 = 0.8;

/// The hours a closed day must cover to be settled.
const SETTLE_HOURS: f64 = 24.0 * SETTLE_COVERAGE;

/// Money and energy of one local day: what it cost with the battery, what it would have cost
/// without one, and the four-way split of the difference.
///
/// The split is exact: `arbitrage + self_consumption + feed_in + battery_balance == saving` at the
/// ledger's 1e-6 resolution (the balance term absorbs the storage rounding). Energy sums cover the
/// measured slots; `coverage_hours` says how much of the day that was.
#[derive(Debug, Clone, PartialEq)]
pub struct DayAccounting {
    pub day: String,
    pub actual_eur: f64,
    pub baseline_eur: f64,
    pub saving_eur: f64,
    pub arbitrage_eur: f64,
    pub self_consumption_eur: f64,
    pub feed_in_eur: f64,
    pub battery_balance_eur: f64,
    pub import_kwh: f64,
    pub export_kwh: f64,
    pub pv_kwh: f64,
    pub battery_charge_kwh: f64,
    pub battery_discharge_kwh: f64,
    pub coverage_hours: f64,
    pub complete: bool,
    /// Whether every measured slot had a published price, so the money is computable at all.
    pub prices_complete: bool,
}

impl DayAccounting {
    /// The ledger row to store.
    ///
    /// A closed day (`closed`) that is not complete stores `NULL` money and keeps its measured
    /// energy and coverage: an incomplete measurement is never turned into a saving. The current
    /// day is provisional and keeps its measured-so-far money.
    pub fn row(&self, closed: bool) -> SavingsDaily {
        let mut row = SavingsDaily::incomplete(self.day.clone());
        if !closed
            || self.complete
            || (self.prices_complete && self.coverage_hours + 1e-9 >= SETTLE_HOURS)
        {
            let actual = round(self.actual_eur);
            let baseline = round(self.baseline_eur);
            let saving = round(baseline - actual);
            let arbitrage = round(self.arbitrage_eur);
            let self_consumption = round(self.self_consumption_eur);
            let feed_in = round(self.feed_in_eur);
            row.actual_eur = Some(actual);
            row.baseline_eur = Some(baseline);
            row.saving_eur = Some(saving);
            row.arbitrage_eur = Some(arbitrage);
            row.self_consumption_eur = Some(self_consumption);
            row.feed_in_eur = Some(feed_in);
            // The balance term carries the split's own 1e-6 reconciliation, so the stored components
            // add up to the stored saving exactly.
            row.battery_balance_eur = Some(round(saving - arbitrage - self_consumption - feed_in));
        }
        row.import_kwh = self.import_kwh;
        row.export_kwh = self.export_kwh;
        row.pv_kwh = self.pv_kwh;
        row.battery_charge_kwh = self.battery_charge_kwh;
        row.battery_discharge_kwh = self.battery_discharge_kwh;
        row.coverage_hours = self.coverage_hours;
        row.complete = self.complete;
        row
    }
}

/// Money at the ledger's 1e-6 resolution.
fn round(value: f64) -> f64 {
    if value.is_finite() {
        (value * 1_000_000.0).round() / 1_000_000.0
    } else {
        0.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::DayAccounting;

    fn day(coverage_hours: f64, complete: bool) -> DayAccounting {
        DayAccounting {
            day: "2026-11-10".to_owned(),
            actual_eur: 3.0,
            baseline_eur: 5.0,
            saving_eur: 2.0,
            arbitrage_eur: 0.5,
            self_consumption_eur: 1.5,
            feed_in_eur: 0.0,
            battery_balance_eur: 0.0,
            import_kwh: 10.0,
            export_kwh: 0.0,
            pv_kwh: 4.0,
            battery_charge_kwh: 6.0,
            battery_discharge_kwh: 5.0,
            coverage_hours,
            complete,
            prices_complete: true,
        }
    }

    #[test]
    fn a_closed_day_with_a_hole_still_stores_its_measured_saving() {
        // 23 of 24 hours: one quarter-hour missing. It used to be null for ever.
        let row = day(23.0, false).row(true);
        assert_eq!(row.saving_eur, Some(2.0));
        assert!(
            !row.complete,
            "still reported as partial, with its coverage"
        );
        assert_eq!(row.coverage_hours, 23.0);
    }

    #[test]
    fn a_closed_day_too_sparse_to_price_stays_null() {
        let row = day(10.0, false).row(true);
        assert_eq!(row.saving_eur, None);
        assert_eq!(row.coverage_hours, 10.0);
    }

    #[test]
    fn the_open_day_is_always_provisional() {
        let row = day(6.0, false).row(false);
        assert_eq!(row.saving_eur, Some(2.0));
    }
}
