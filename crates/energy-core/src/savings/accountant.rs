//! Replaying one measured day twice: with the battery (actual) and without it (baseline).

use chrono::Datelike;
use energy_price::{ExportSource, Settlement, Slot};

use super::{DayAccounting, DayReading, SettlementSeed, SlotFlow};

/// Account one measured local day against its no-battery counterfactual.
///
/// Both legs replay the day in slot order from the same start-of-day saldering counters: each
/// advances its own import meter and consumes the budget with its own exports. That is the coupling
/// approximation documented in `docs/SAVINGS.md` — the counterfactual's year is not replayed, only
/// its day. Flows are never zero-filled: a slot with a missing measurement is skipped, and a missing
/// price leaves the day unsettled (and therefore incomplete) while its measured energy is kept.
pub fn account(reading: &DayReading, template: &Settlement) -> DayAccounting {
    let year = reading.measured.day.year();
    let reference = reading.reference_price();
    let mut replay = Replay::new(template, reading.seed, year);
    let mut missing_price = false;
    for flow in &reading.measured.slots {
        if !flow.is_measured() {
            continue;
        }
        match reading.price(flow.t_start) {
            Some(price) => replay.slot(flow, price, reference),
            None => missing_price = true,
        }
    }
    replay.finish(reading, missing_price)
}

/// The two legs and the accumulators of one replay.
struct Replay {
    actual: Settlement,
    baseline: Settlement,
    year: i32,
    actual_import_ytd: f64,
    baseline_import_ytd: f64,
    actual_cost: f64,
    baseline_cost: f64,
    actual_revenue: f64,
    baseline_revenue: f64,
    arbitrage: f64,
    self_consumption: f64,
    feed_in: f64,
    balance: f64,
}

impl Replay {
    fn new(template: &Settlement, seed: SettlementSeed, year: i32) -> Self {
        let mut actual = template.clone();
        let mut baseline = template.clone();
        seed.apply(&mut actual, year);
        seed.apply(&mut baseline, year);
        Self {
            actual,
            baseline,
            year,
            actual_import_ytd: seed.import_ytd_kwh(),
            baseline_import_ytd: seed.import_ytd_kwh(),
            actual_cost: 0.0,
            baseline_cost: 0.0,
            actual_revenue: 0.0,
            baseline_revenue: 0.0,
            arbitrage: 0.0,
            self_consumption: 0.0,
            feed_in: 0.0,
            balance: 0.0,
        }
    }

    /// Settle one measured and priced slot on both legs and book the split.
    ///
    /// Settlement is on the slot **net** (prices.md §4.3): a slot whose meter registers both
    /// imported and exported energy is one net flow, so the two registers cannot each be priced. The
    /// baseline settles the net flow a battery-less house would have had (`load - pv`); the actual
    /// leg settles the measured net (`import - export`). Import is counted before export in every
    /// slot, so the budget an export can net against includes the same slot's import.
    ///
    /// The self-consumption and feed-in terms use the same net decomposition, so the four
    /// components still add up to `baseline - actual` exactly.
    fn slot(&mut self, flow: &SlotFlow, price: &Slot, reference: f64) {
        let load = flow.load_kwh().unwrap_or_default();
        let import = flow.import_kwh.unwrap_or_default();
        let export = flow.export_kwh.unwrap_or_default();
        let pv = flow.pv_kwh.unwrap_or_default();
        let charge = flow.charge_kwh.unwrap_or_default();
        let discharge = flow.discharge_kwh.unwrap_or_default();
        let buy = price.buy_all_in;
        let actual_import = (import - export).max(0.0);
        let actual_export = (export - import).max(0.0);
        let base_import = (load - pv).max(0.0);
        let base_export = (pv - load).max(0.0);

        self.actual_import_ytd += actual_import;
        self.actual
            .set_import_ytd(self.year, self.actual_import_ytd);
        self.actual_cost += actual_import * buy;
        let actual_revenue = self
            .actual
            .settle(price, actual_export, export_source(flow));
        self.actual_revenue += actual_revenue;

        self.baseline_import_ytd += base_import;
        self.baseline
            .set_import_ytd(self.year, self.baseline_import_ytd);
        self.baseline_cost += base_import * buy;
        let baseline_revenue = self.baseline.settle(price, base_export, ExportSource::Pv);
        self.baseline_revenue += baseline_revenue;

        self.arbitrage += (discharge - charge) * (buy - reference);
        self.balance += (discharge - charge) * reference;
        self.self_consumption += (base_export - actual_export) * buy;
        self.feed_in += actual_revenue - baseline_revenue;
    }

    /// Turn the accumulators into the day's row.
    fn finish(&self, reading: &DayReading, missing_price: bool) -> DayAccounting {
        let actual = self.actual_cost - self.actual_revenue;
        let baseline = self.baseline_cost - self.baseline_revenue;
        let totals = reading.measured.totals();
        DayAccounting {
            day: reading.measured.day.to_string(),
            actual_eur: actual,
            baseline_eur: baseline,
            saving_eur: baseline - actual,
            arbitrage_eur: self.arbitrage,
            self_consumption_eur: self.self_consumption,
            feed_in_eur: self.feed_in,
            battery_balance_eur: self.balance,
            import_kwh: totals.import_kwh,
            export_kwh: totals.export_kwh,
            pv_kwh: totals.pv_kwh,
            battery_charge_kwh: totals.charge_kwh,
            battery_discharge_kwh: totals.discharge_kwh,
            coverage_hours: reading.measured.coverage_hours(),
            complete: reading.measured.is_complete() && !missing_price,
        }
    }
}

/// Which side of the meter the measured export came from: PV export earns the Zonnebonus, battery
/// export does not.
fn export_source(flow: &SlotFlow) -> ExportSource {
    if flow.pv_kwh.unwrap_or_default() > 0.0 {
        ExportSource::Pv
    } else {
        ExportSource::Battery
    }
}
