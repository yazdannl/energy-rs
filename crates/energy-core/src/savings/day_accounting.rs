//! The result of accounting one measured local day.

use energy_ledger::SavingsDaily;

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
}

impl DayAccounting {
    /// The ledger row to store.
    ///
    /// A closed day (`closed`) that is not complete stores `NULL` money and keeps its measured
    /// energy and coverage: an incomplete measurement is never turned into a saving. The current
    /// day is provisional and keeps its measured-so-far money.
    pub fn row(&self, closed: bool) -> SavingsDaily {
        let mut row = SavingsDaily::incomplete(self.day.clone());
        if !closed || self.complete {
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
