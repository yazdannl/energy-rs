//! Shared fixtures for the P8a integration tests.

#![allow(dead_code)]

use chrono::NaiveDate;
use energy_core::plan::{PackSpec, PackState, Plan, PlanSummary};
use energy_core::safety::{CommandRequest, Measured};
use energy_ledger::plan::PlanSlot;
use energy_price::book::SlotSource;
use energy_price::{Contract, PriceBook, Settlement, Slot};

/// A deterministic plan instant on a quarter-hour boundary.
pub const NOW: i64 = 1_800_000_000;

pub fn spec(id: &str, capacity_kwh: f64, max_w: f64) -> PackSpec {
    PackSpec {
        pack_id: id.to_owned(),
        capacity_kwh,
        min_soc_pct: 5.0,
        max_soc_pct: 95.0,
        max_charge_w: max_w,
        max_discharge_w: max_w,
        eta_charge: 0.95,
        eta_discharge: 0.95,
    }
}

pub fn state(id: &str, capacity_kwh: f64, max_w: f64, soc_pct: f64) -> PackState {
    PackState {
        spec: spec(id, capacity_kwh, max_w),
        soc_pct,
    }
}

pub fn settlement() -> Settlement {
    let date = NaiveDate::from_ymd_opt(2027, 1, 1).expect("valid date");
    Settlement::on(Contract::zonneplan_2026(), date)
}

/// A contiguous book of `count` quarter-hour slots from `start`.
pub fn book(start: i64, count: usize, buy: impl Fn(usize) -> f64) -> PriceBook {
    let slots: Vec<Slot> = (0..count)
        .map(|index| {
            Slot::new(
                start + i64::try_from(index).unwrap() * 900,
                buy(index),
                0.08,
                SlotSource::ZonneplanQuarterHourly,
            )
        })
        .collect();
    PriceBook::new(slots).expect("contiguous book")
}

pub fn flat_book(start: i64, count: usize) -> PriceBook {
    book(start, count, |_| 0.20)
}

pub fn forecasts(value: f64, count: usize) -> Vec<f64> {
    vec![value; count]
}

pub fn request(pack_id: &str, requested_w: f64, now: i64) -> CommandRequest {
    CommandRequest::new(pack_id, requested_w, None, now, Measured::default())
}

pub fn plan_slot(now: i64, power_kw: f64, soc_kwh: f64) -> PlanSlot {
    let mut slot = PlanSlot::at(0, now);
    slot.buy_all_in = 0.20;
    slot.feed_in = 0.05;
    slot.pv_kw = 0.0;
    slot.load_kw = 0.5;
    slot.battery_kw = power_kw;
    slot.soc_kwh = soc_kwh;
    slot.target_soc_kwh = soc_kwh;
    "test".clone_into(&mut slot.reason);
    slot
}

pub fn summary() -> PlanSummary {
    PlanSummary {
        cost_estimate_eur: 0.0,
        baseline_cost_eur: 0.0,
        saved_eur: 0.0,
        charged_kwh: 0.0,
        discharged_kwh: 0.0,
        grid_charge_kwh: 0.0,
        pv_charge_kwh: 0.0,
        cycles: 0.0,
        terminal_target_kwh: None,
        infeasible_reason: None,
    }
}

pub fn plan(generated_at: i64, slot_soc_kwh: f64) -> Plan {
    let mut slot = plan_slot(generated_at, 0.0, slot_soc_kwh);
    "reserve_hold".clone_into(&mut slot.reason);
    Plan {
        generated_at,
        horizon_start: generated_at,
        horizon_end: generated_at + 120 * 900,
        feasible: true,
        objective_eur: 0.0,
        sell_floor: None,
        summary: summary(),
        slots: vec![slot],
    }
}
