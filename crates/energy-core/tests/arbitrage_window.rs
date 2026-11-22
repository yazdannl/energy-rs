//! The 2026-11-22 incident: a sunless horizon with a cheap night and a dear next-day peak.
//!
//! The solver found the overnight charge, but the solar-reserve ceiling threw it away before it
//! reached the timeline, so the plan sold once at the peak and never bought back. These tests pin
//! the fixed behaviour: buy the valley, sell the peak, finish at the hard reserve floor; and when
//! two future peaks pay the same, do not park the whole sale on the later one.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use std::sync::{Arc, Mutex};

use energy_battery::protection::full_charge::FullChargeSettings;
use energy_core::dispatch::BoxFuture;
use energy_core::engine::PlanSource;
use energy_core::maintenance::{LedgerFullChargeHistory, MaintenancePort, apply, candidates};
use energy_core::plan::{
    ExportPolicy, HORIZON_SLOTS, PackSpec, PackState, Plan, PlanError, Policy,
};
use energy_core::plan_source::{
    Forecast, LivePlanSource, MeasuredHistory, PlanSupply, SourceConfig, Supply,
};
use energy_price::settlement::NetMetering;
use energy_price::{Contract, PriceBook, Settlement};
use support::NOW;

/// Four learned AB2000s: 7.86 kWh, 4 kW charge / 3.2 kW discharge, 90 % each way.
const FLEET_KWH: f64 = 4.0 * 1.965;

struct FakeSupply {
    supply: Mutex<Supply>,
}

impl PlanSupply for FakeSupply {
    fn supply(&self, _now: i64) -> BoxFuture<'_, Result<Supply, PlanError>> {
        Box::pin(async move { Ok(self.supply.lock().expect("supply").clone()) })
    }
}

fn packs(soc_pct: f64) -> Vec<PackState> {
    (0..4)
        .map(|index| {
            let spec = PackSpec {
                pack_id: format!("pack-{index}"),
                capacity_kwh: 1.965,
                min_soc_pct: 5.0,
                max_soc_pct: 100.0,
                max_charge_w: 1000.0,
                max_discharge_w: 800.0,
                eta_charge: 0.9,
                eta_discharge: 0.9,
            };
            PackState { spec, soc_pct }
        })
        .collect()
}

fn supply(prices: PriceBook, pv_kw: Vec<f64>) -> Supply {
    let starts: Vec<i64> = (0..96).map(|index| NOW - 900 * (96 - index)).collect();
    // 1:1 net metering: sell in the peak, buy back in the valley at the same all-in tariff.
    let mut settlement = Settlement::on(Contract::zonneplan_2026(), support::settlement().today());
    settlement.set_regime(NetMetering::On);
    Supply {
        history: MeasuredHistory::new(starts, vec![0.0; 96], vec![0.65; 96]).expect("history"),
        forecast: Some(Forecast::fresh(pv_kw, vec![0.65; HORIZON_SLOTS], NOW)),
        prices,
        price_fetched_at: NOW,
        settlement,
    }
}

fn source_with_pv(prices: PriceBook, pv_kw: Vec<f64>) -> LivePlanSource<FakeSupply> {
    // The live settings: 15 % hard reserve, 10 % solar headroom, 10 % soft peak guideline (which
    // sits below the hard floor, so the effective terminal target is the floor), 2c wear.
    let policy = Policy {
        export: ExportPolicy::Legacy,
        net_metering: NetMetering::On,
        solar_reserve_pct: 10.0,
        peak_reserve_pct: 10.0,
        peak_reserve_value_eur_per_kwh: 0.15,
        wear_eur_per_kwh: 0.02,
        cost_basis_eur_per_kwh: None,
        zero_export: false,
    };
    let config = SourceConfig {
        reserve_soc_pct: 15.0,
        policy,
        ..SourceConfig::default()
    };
    LivePlanSource::new(
        FakeSupply {
            supply: Mutex::new(supply(prices, pv_kw)),
        },
        config,
    )
}

fn source(prices: PriceBook) -> LivePlanSource<FakeSupply> {
    source_with_pv(prices, vec![0.0; HORIZON_SLOTS])
}

fn book(price: impl Fn(usize) -> f64) -> PriceBook {
    support::book(NOW, HORIZON_SLOTS, price)
}

/// Packs whose last completed balance was ten days ago: the seven-day rule is due.
fn due_port(fleet: &[PackState]) -> MaintenancePort {
    let ids: Vec<&str> = fleet
        .iter()
        .map(|pack| pack.spec.pack_id.as_str())
        .collect();
    let ledger = support::fakes::ledger_with(&ids);
    for pack in fleet {
        ledger
            .packs()
            .unwrap()
            .upsert(&energy_ledger::Pack {
                last_full_charge_at: Some(NOW - 10 * 86_400),
                ..energy_ledger::Pack::new(&pack.spec.pack_id, 1.965, NOW)
            })
            .unwrap();
    }
    MaintenancePort::new(
        Arc::new(LedgerFullChargeHistory::new(ledger)),
        FullChargeSettings {
            enabled: true,
            interval_days: 7,
            dwell_minutes: 60,
            ..FullChargeSettings::weekly(800)
        },
    )
}

fn slots_at(plan: &Plan, range: std::ops::Range<usize>) -> f64 {
    plan.slots[range]
        .iter()
        .filter(|slot| slot.battery_kw < 0.0)
        .map(|slot| -slot.battery_kw * 0.25)
        .sum()
}

/// Gross saving, wear and net profit, recomputed from the plan's grid flows and real tariffs.
///
/// The C core quantises energy to a 0.05 kWh SoC lattice, power to 24 actions per slot and prices
/// to 1e-6 EUR; its value function is linearly interpolated between SoC nodes. Those quanta, not
/// an arbitrary allowance, justify the 0.01 kWh / 0.01 EUR tolerances below.
fn economics(plan: &Plan) -> (f64, f64, f64) {
    let mut cost = 0.0;
    let mut baseline = 0.0;
    let mut wear = 0.0;
    for slot in &plan.slots {
        let base = slot.load_kw - slot.pv_kw;
        let grid = base + slot.battery_kw;
        cost += 0.25
            * if grid >= 0.0 {
                slot.buy_all_in * grid
            } else {
                slot.feed_in * grid
            };
        baseline += 0.25 * slot.buy_all_in * base;
        if slot.battery_kw < 0.0 {
            // 0.02 EUR per stored kWh discharged; AC energy / eta_discharge = stored energy.
            wear += 0.02 * (-slot.battery_kw * 0.25) / 0.9;
        }
    }
    let saved = baseline - cost;
    (saved, wear, saved - wear)
}

#[tokio::test]
async fn a_sunless_cheap_night_charges_and_the_next_peak_is_sold() {
    // Real 2026-11-22 shape: 0.35 overnight, ~0.40 in between, 0.51-0.53 tomorrow afternoon.
    let prices = book(|index| {
        if index < 16 {
            [0.3498, 0.3511, 0.3526, 0.3503][index / 4]
        } else if (80..96).contains(&index) {
            [0.5099, 0.5114, 0.5303, 0.5188][(index - 80) / 4]
        } else {
            0.40
        }
    });
    let plan = source(prices).plan(NOW, packs(92.0)).await.expect("plan");

    let cheap_charge: f64 = plan.slots[..16]
        .iter()
        .filter(|slot| slot.battery_kw > 0.0)
        .map(|slot| slot.battery_kw * 0.25)
        .sum();
    // Independent DP with the live fleet: 0.6987 kWh bought at 0.3498, 6.0129 kWh sold in the
    // peak (4 x 0.8 kWh at 0.5303, 2 x 0.8 kWh plus 1.2129 kWh at 0.5188), ending on the 1.179
    // kWh floor. All three energies are pinned to 0.01 kWh.
    assert!(
        (cheap_charge - 0.6987).abs() < 0.01,
        "cheap charge {cheap_charge} != 0.6987 kWh"
    );
    assert!(
        (plan.summary.grid_charge_kwh - 0.6987).abs() < 0.01,
        "grid charge was clipped: {}",
        plan.summary.grid_charge_kwh
    );
    let peak_sale = slots_at(&plan, 80..96);
    assert!(
        (peak_sale - 6.0129).abs() < 0.01,
        "peak sale {peak_sale} != 6.0129 kWh"
    );
    assert!(
        (plan.summary.discharged_kwh - peak_sale).abs() < 1e-9,
        "sold energy outside the peak"
    );
    let terminal = plan.slots.last().expect("last slot").target_soc_kwh;
    assert!(
        (terminal - 1.179).abs() < 0.01,
        "ended at {terminal}, expected the 1.179 kWh reserve floor"
    );

    // By hand: baseline 23.4 kWh of house load costs 9.53823 EUR (4 h at ~0.350, 16 h at 0.40,
    // 4 h at ~0.518, 12 h at 0.40). The plan buys 0.6987 kWh for 0.24439 EUR and sells 6.0129 kWh
    // for 3.15629 EUR, so gross saving is 2.91190 EUR. Wear is 0.02 x 6.0129/0.9 = 0.13362 EUR,
    // leaving 2.77828 EUR net.
    let (saved, wear, net) = economics(&plan);
    assert!((plan.summary.baseline_cost_eur - 9.53823).abs() < 0.01);
    assert!((saved - 2.91190).abs() < 0.01, "gross saving {saved}");
    assert!((wear - 0.13362).abs() < 0.005, "wear {wear}");
    assert!((net - 2.77828).abs() < 0.01, "net profit {net}");
}

#[tokio::test]
async fn an_equal_future_peak_is_not_skipped_for_a_later_twin() {
    // Two equally dear peaks, both still in the future, with no profitable valley between them.
    // Energy is finite: selling into the first peak must not be deferred to the identical later
    // one. The first window therefore carries at least as much discharge as the second.
    let prices = book(|index| {
        if (40..48).contains(&index) || (80..88).contains(&index) {
            0.5303
        } else {
            0.45
        }
    });
    let plan = source(prices).plan(NOW, packs(92.0)).await.expect("plan");

    // The C core's strict `<` on ascending actions resolves equal value to the lower power, so
    // the still-future peak takes 4.5348 kWh and only 0.9121 kWh is left for the identical twin;
    // total sale 5.4470 kWh. Pin all three to 0.01 kWh instead of accepting any positive sale.
    let first = slots_at(&plan, 40..48);
    let later = slots_at(&plan, 80..88);
    assert!(
        (first - 4.5348).abs() < 0.01,
        "first peak sale {first} != 4.5348 kWh"
    );
    assert!(
        (later - 0.9121).abs() < 0.01,
        "later peak sale {later} != 0.9121 kWh"
    );
    assert!(
        (plan.summary.discharged_kwh - 5.4470).abs() < 0.01,
        "total sale {} != 5.4470 kWh",
        plan.summary.discharged_kwh
    );
    assert!(
        first > later,
        "tie-break deferred the sale to the later twin"
    );
    assert!(
        plan.slots[40..48]
            .iter()
            .chain(&plan.slots[80..88])
            .all(|slot| slot.battery_kw <= 0.0),
        "a peak slot charged instead of selling"
    );

    // By hand: house load costs 10.73878 EUR (32 h at 0.45 plus 4 h at 0.5303); all 5.4470 kWh
    // of sale earn 0.5303, i.e. 2.88853 EUR. Wear is 0.02 x 5.4470/0.9 = 0.12104 EUR, leaving
    // 2.76749 EUR net. This fails if the same throughput is achieved at a different tariff.
    let (saved, wear, net) = economics(&plan);
    assert!((saved - 2.88853).abs() < 0.01, "gross saving {saved}");
    assert!((wear - 0.12104).abs() < 0.005, "wear {wear}");
    assert!((net - 2.76749).abs() < 0.01, "net profit {net}");
}

#[tokio::test]
async fn a_due_seven_day_balance_wins_over_a_live_solar_reserve_ceiling() {
    // PV surplus in the horizon keeps the 10 % solar reserve armed: the economic plan may not
    // grid-charge above 7.074 kWh. A pack that has not balanced for ten days still gets its
    // 7-day full charge, folded into the plan after the clamp.
    let prices = book(|index| if index < 16 { 0.20 } else { 0.40 });
    let pv = vec![1.0; HORIZON_SLOTS]; // 0.35 kW surplus every slot
    let mut plan = source_with_pv(prices, pv)
        .plan(NOW, packs(92.0))
        .await
        .expect("plan");
    assert_eq!(
        plan.summary.grid_charge_kwh, 0.0,
        "clamp should hold the reserve here"
    );

    let fleet = packs(92.0);
    let port = due_port(&fleet);
    let candidates = candidates(&plan, NOW, 5_000.0);
    let assignments = port.assignments(NOW, &fleet, &candidates, 5_000.0);
    assert_eq!(
        assignments.assignments.len(),
        4,
        "{:?}",
        assignments.unassigned
    );
    apply(&mut plan, &assignments.assignments, FLEET_KWH, 0.9);

    let booked: Vec<_> = plan
        .slots
        .iter()
        .filter(|slot| slot.reason.contains("maintenance_full_charge"))
        .collect();
    assert!(!booked.is_empty(), "maintenance did not win over the clamp");
    assert!(booked.iter().all(|slot| slot.battery_kw > 0.0));
    let end_soc = booked.last().expect("booked slot").target_soc_kwh / FLEET_KWH * 100.0;
    assert!(
        (99.0..=100.0).contains(&end_soc),
        "maintenance did not reach the balancing target: {end_soc}"
    );
}

#[tokio::test]
async fn the_import_ceiling_covers_planner_charge_plus_maintenance_charge() {
    let prices = book(|index| {
        if index < 16 {
            [0.3498, 0.3511, 0.3526, 0.3503][index / 4]
        } else if (80..96).contains(&index) {
            [0.5099, 0.5114, 0.5303, 0.5188][(index - 80) / 4]
        } else {
            0.40
        }
    });
    let mut plan = source(prices).plan(NOW, packs(92.0)).await.expect("plan");
    let planner_charge = plan.summary.grid_charge_kwh;
    assert!(
        planner_charge > 0.5,
        "need a real planner charge to share the budget"
    );

    let fleet = packs(92.0);
    let port = due_port(&fleet);
    let budget_w = 4_500.0;
    let candidates = candidates(&plan, NOW, budget_w);
    // The corrected clamp scheduled 3.238 kW at 02:45Z on top of 0.65 kW of house load: only
    // the remaining ~0.6 kW may be offered to maintenance, never the whole ceiling again.
    let charge_slot = plan
        .slots
        .iter()
        .filter(|slot| slot.battery_kw > 0.0)
        .max_by(|a, b| a.battery_kw.total_cmp(&b.battery_kw))
        .expect("planner charge slot");
    let planner_w = charge_slot.battery_kw * 1000.0;
    let candidate = candidates
        .iter()
        .find(|slot| slot.slot_start == charge_slot.t_start)
        .expect("matching candidate");
    assert!(
        (candidate.available_w - (budget_w - 650.0 - planner_w)).abs() < 1.0,
        "maintenance budget did not deduct the planner charge"
    );
    let assignments = port.assignments(NOW, &fleet, &candidates, budget_w);
    apply(&mut plan, &assignments.assignments, FLEET_KWH, 0.9);
    for slot in &plan.slots {
        let grid_w = slot.load_kw * 1000.0 + slot.battery_kw.max(0.0) * 1000.0;
        assert!(
            grid_w <= budget_w + 1e-6,
            "planner + maintenance import {grid_w} W exceeds {budget_w} W"
        );
    }
    assert!(plan.summary.grid_charge_kwh > planner_charge);
}
