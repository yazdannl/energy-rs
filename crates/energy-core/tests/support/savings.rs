//! Scripted savings days, a scripted source and the split assertions.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, NaiveTime};
use energy_core::dispatch::BoxFuture;
use energy_core::savings::{
    DayAccounting, DayReading, MeasuredDay, SavingsError, SavingsSource, SettlementSeed, SlotFlow,
};
use energy_ledger::{Ledger, SavingsDaily};
use energy_price::book::SlotSource;
use energy_price::book::query::{local_midnight, local_naive_to_utc};
use energy_price::{Contract, Settlement, Slot};

/// The test contract: Zonneplan 2026 constants with the Zonnebonus switched off, so the feed-in
/// price is exactly `market + 0.02 / 1.21` (the supplier fee is 2 ct incl. VAT, an exported kWh
/// carries no VAT) and the scenarios stay readable.
pub fn contract() -> Contract {
    let mut contract = Contract::zonneplan_2026();
    contract.bonus.pct = 0.0;
    contract
}

/// A settlement for one local day (the date drives saldering on/off).
pub fn settlement(day: &str) -> Settlement {
    Settlement::on(contract(), parse_day(day))
}

pub fn parse_day(day: &str) -> NaiveDate {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").expect("a valid ISO day")
}

/// Noon locally; the rollup's "now" for a test day.
pub fn noon(day: &str) -> i64 {
    local_naive_to_utc(parse_day(day).and_time(NaiveTime::from_hms_opt(12, 0, 0).expect("noon")))
        .expect("a representable local noon")
}

/// Every quarter-hour of a local day (92, 96 or 100 depending on DST).
pub fn slots_of(day: NaiveDate) -> Vec<i64> {
    let from = local_midnight(day).expect("local midnight");
    let to = local_midnight(day.succ_opt().expect("a next day")).expect("next midnight");
    let mut starts = Vec::new();
    let mut t_start = from;
    while t_start < to {
        starts.push(t_start);
        t_start += 900;
    }
    starts
}

/// A day reading whose flows are physically consistent: `import - export = load + charge - pv -
/// discharge` per slot, so the counterfactual identity always holds.
pub fn reading(
    day: &str,
    buy: impl Fn(usize) -> f64,
    market: impl Fn(usize) -> f64,
    plan: impl Fn(usize) -> (f64, f64, f64, f64),
    seed: SettlementSeed,
) -> DayReading {
    let date = parse_day(day);
    let starts = slots_of(date);
    let prices: BTreeMap<i64, Slot> = starts
        .iter()
        .enumerate()
        .map(|(index, &t_start)| {
            let slot = Slot::new(
                t_start,
                buy(index),
                market(index),
                SlotSource::ZonneplanQuarterHourly,
            );
            (t_start, slot)
        })
        .collect();
    let slots = starts
        .iter()
        .enumerate()
        .map(|(index, &t_start)| {
            let (pv, load, charge, discharge) = plan(index);
            let net = load + charge - pv - discharge;
            SlotFlow::measured(
                t_start,
                net.max(0.0),
                (-net).max(0.0),
                pv,
                charge,
                discharge,
            )
        })
        .collect();
    DayReading {
        measured: MeasuredDay::new(date, slots),
        prices,
        seed,
    }
}

/// A seed with a saldering budget (import metered, exports already netted) and no bonus history.
pub fn seeded(year: i32, import_ytd_kwh: f64, netted_ytd_kwh: f64) -> SettlementSeed {
    SettlementSeed::new(
        Some(energy_price::Saldering::new(
            import_ytd_kwh,
            netted_ytd_kwh,
            year,
        )),
        0.0,
    )
}

/// A seed without any budget (net metering off, or no meter history).
pub fn unseeded() -> SettlementSeed {
    SettlementSeed::new(None, 0.0)
}

/// The four components add up to the saving exactly (in-memory arithmetic).
pub fn assert_split(accounting: &DayAccounting) {
    let sum = accounting.arbitrage_eur
        + accounting.self_consumption_eur
        + accounting.feed_in_eur
        + accounting.battery_balance_eur;
    assert!(
        (sum - accounting.saving_eur).abs() < 1e-9,
        "split {sum} != saving {}\n{accounting:?}",
        accounting.saving_eur
    );
}

/// The stored row keeps the same exact split at the ledger's 1e-6 resolution.
pub fn assert_row_split(row: &SavingsDaily) {
    let money = |value: Option<f64>| value.expect("money for a complete day");
    let sum = money(row.arbitrage_eur)
        + money(row.self_consumption_eur)
        + money(row.feed_in_eur)
        + money(row.battery_balance_eur);
    assert!(
        (sum - money(row.saving_eur)).abs() < 1e-9,
        "stored split {sum} != stored saving\n{row:?}"
    );
}

/// A source that hands out scripted day readings and counts the reads.
#[derive(Debug, Default)]
pub struct ScriptedSource {
    days: Mutex<BTreeMap<String, DayReading>>,
    reads: AtomicUsize,
}

impl ScriptedSource {
    pub fn new(readings: Vec<DayReading>) -> Arc<Self> {
        let days = readings
            .into_iter()
            .map(|reading| (reading.measured.day.to_string(), reading))
            .collect();
        Arc::new(Self {
            days: Mutex::new(days),
            reads: AtomicUsize::new(0),
        })
    }

    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    /// Remove a day so its read fails (Home Assistant down for that day).
    pub fn drop_day(&self, day: &str) {
        self.days.lock().expect("scripted days").remove(day);
    }

    /// Script one more day after the source was built.
    pub fn add(&self, reading: DayReading) {
        self.days
            .lock()
            .expect("scripted days")
            .insert(reading.measured.day.to_string(), reading);
    }
}

impl SavingsSource for ScriptedSource {
    fn read_day(&self, day: NaiveDate) -> BoxFuture<'_, Result<DayReading, SavingsError>> {
        Box::pin(async move {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let key = day.to_string();
            self.days
                .lock()
                .expect("scripted days")
                .get(&key)
                .cloned()
                .ok_or(SavingsError::Measurement {
                    series: "grid_import",
                    reason: format!("{key} is not scripted"),
                })
        })
    }
}

/// A fresh in-memory ledger.
pub fn ledger() -> Arc<Ledger> {
    Arc::new(Ledger::open_in_memory().expect("in-memory ledger"))
}
