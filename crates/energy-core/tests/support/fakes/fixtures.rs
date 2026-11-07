//! Plans and packs shared by the engine tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]
#![allow(dead_code)]

use std::sync::Arc;

use energy_core::dispatch::PackReading;
use energy_core::plan::{HORIZON_SLOTS, PackSpec, Plan, PlanSummary};
use energy_ledger::plan::PlanSlot;
use energy_ledger::{Ledger, Pack};

/// A quarter-hour boundary: slot index 2 000 000, so `now + 5` stays in the same slot.
pub const NOW: i64 = 1_800_000_000;

/// A pack with the shipped SolarFlow 800 Plus limits.
pub fn pack(id: &str, soc_pct: f64) -> PackReading {
    PackReading {
        ledger_id: None,
        spec: PackSpec {
            pack_id: id.to_owned(),
            capacity_kwh: 1.92,
            min_soc_pct: 5.0,
            max_soc_pct: 95.0,
            max_charge_w: 800.0,
            max_discharge_w: 800.0,
            eta_charge: 0.95,
            eta_discharge: 0.95,
        },
        soc_pct: Some(soc_pct),
        energy_kwh: Some(1.92 * soc_pct / 100.0),
        power_w: 0.0,
        min_cell_v: Some(3.30),
        observed_at: NOW,
        available: true,
        stale: false,
    }
}

/// A 120-slot plan that holds `power_kw` all horizon, with the SoC the packs are at.
pub fn plan(now: i64, power_kw: f64, soc_kwh: f64) -> Plan {
    let start = now - now.rem_euclid(900);
    let slots = (0..HORIZON_SLOTS)
        .map(|index| {
            let offset = i64::try_from(index).unwrap_or(0);
            let mut slot = PlanSlot::at(offset, start + 900 * offset);
            slot.buy_all_in = 0.20;
            slot.feed_in = 0.05;
            slot.pv_kw = 0.0;
            slot.load_kw = 0.5;
            slot.battery_kw = power_kw;
            slot.soc_kwh = soc_kwh;
            slot.target_soc_kwh = soc_kwh + power_kw * 0.25;
            "test".clone_into(&mut slot.reason);
            slot
        })
        .collect();
    Plan {
        generated_at: now,
        horizon_start: start,
        horizon_end: start + 900 * i64::try_from(HORIZON_SLOTS).unwrap_or(0),
        feasible: true,
        objective_eur: 0.0,
        sell_floor: Some(0.05),
        summary: summary(),
        degraded: None,
        slots,
    }
}

fn summary() -> PlanSummary {
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

/// An in-memory ledger pre-loaded with one pack row per id.
pub fn ledger_with(pack_ids: &[&str]) -> Arc<Ledger> {
    let ledger = Arc::new(Ledger::open_in_memory().expect("in-memory ledger"));
    {
        let packs = ledger.packs().expect("packs");
        for id in pack_ids {
            packs.upsert(&Pack::new(*id, 1.92, NOW)).expect("pack row");
        }
    }
    ledger
}
