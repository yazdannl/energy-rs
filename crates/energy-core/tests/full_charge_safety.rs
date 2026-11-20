//! Safety and API edges of the balancing rule: import ceiling, SoC ceiling, restart and reporting.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::assert_is_empty
)]

mod support;

use std::sync::Arc;

use energy_battery::protection::full_charge::{
    ChargeCandidate, DuePack, FullChargeSettings, UnassignedPack,
};
use energy_core::dispatch::{FleetView, MeterReading};
use energy_core::maintenance::{
    FullChargeHistory, LedgerFullChargeHistory, MaintenancePort, apply, candidates,
};
use energy_core::plan::SLOT_SECONDS;
use energy_core::safety::Authority;
use support::fakes::{self, NOW, pack, plan};
use support::harness;

fn settings() -> FullChargeSettings {
    FullChargeSettings {
        enabled: true,
        interval_days: 7,
        dwell_minutes: 60,
        ..FullChargeSettings::weekly(800)
    }
}

fn due_pack(pack_id: &str, soc: f64, last_full_at: Option<i64>, forced: bool) -> DuePack {
    DuePack {
        pack_id: pack_id.into(),
        capacity_kwh: 1.92,
        soc_pct: soc,
        max_soc_pct: 100.0,
        eta_charge: 0.95,
        max_charge_w: 800.0,
        last_full_at,
        forced,
    }
}

fn slots(prices: &[f64], available_w: f64) -> Vec<ChargeCandidate> {
    prices
        .iter()
        .enumerate()
        .map(|(index, price)| ChargeCandidate {
            slot_start: NOW + index as i64 * SLOT_SECONDS,
            t_start: NOW + index as i64 * SLOT_SECONDS,
            t_end: NOW + (index as i64 + 1) * SLOT_SECONDS,
            price_eur_per_kwh: *price,
            available_w,
        })
        .collect()
}

#[test]
fn a_soc_ceiling_below_the_target_is_refused_not_reported_as_complete() {
    let mut pack = due_pack("ONE", 13.0, None, false);
    pack.max_soc_pct = 95.0;
    let result = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &slots(&[0.08; 40], 1_000.0),
        &[pack],
        7_500.0,
    );
    assert!(result.assignments.is_empty());
    assert_eq!(result.unassigned.len(), 1);
    assert_eq!(result.unassigned[0].reason, "soc_ceiling_below_target");
}

#[test]
fn four_packs_share_a_load_spiked_import_budget_without_breaching_it() {
    let mut source = plan(NOW, 0.0, 1.92);
    for slot in &mut source.slots {
        slot.buy_all_in = 0.08;
        slot.load_kw = 6.5; // 1 kW of headroom under the 7.5 kW production budget.
        slot.pv_kw = 0.0;
        slot.battery_kw = 0.0;
    }
    let candidates = candidates(&source, NOW, 7_500.0);
    assert!(
        candidates
            .iter()
            .all(|slot| slot.available_w <= 1_000.0 + 1e-6)
    );
    let packs: Vec<DuePack> = (0..4)
        .map(|index| due_pack(&format!("PACK-{index}"), 13.0, None, false))
        .collect();
    let result = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &packs,
        7_500.0,
    );
    assert_eq!(result.assignments.len(), 4, "{:?}", result.unassigned);
    let mut used = std::collections::BTreeMap::new();
    for assignment in &result.assignments {
        for slot in &assignment.slots {
            assert!(slot.charge_w <= 800.0 + 1e-6, "per-pack ceiling breached");
            let entry = used.entry(slot.slot_start).or_insert(0.0);
            *entry += slot.charge_w;
            assert!(
                *entry <= 1_000.0 + 1e-6,
                "budget breached in {}",
                slot.slot_start
            );
        }
    }
    apply(&mut source, &result.assignments, 1.92, 0.9);
    for slot in &source.slots {
        let grid_w = slot.load_kw * 1000.0 + slot.battery_kw * 1000.0;
        assert!(
            grid_w <= 7_500.0 + 1e-6,
            "folded import {grid_w} W exceeds budget"
        );
    }
}

#[test]
fn an_impossible_house_load_reports_no_headroom_instead_of_planning() {
    let mut source = plan(NOW, 0.0, 1.92);
    for slot in &mut source.slots {
        slot.buy_all_in = 0.08;
        slot.load_kw = 7.6;
        slot.pv_kw = 0.0;
        slot.battery_kw = 0.0;
    }
    let candidates = candidates(&source, NOW, 7_500.0);
    assert!(candidates.iter().all(|slot| slot.available_w == 0.0));
    let result = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &[due_pack("ONE", 13.0, None, false)],
        7_500.0,
    );
    assert!(result.assignments.is_empty());
    assert_eq!(result.unassigned[0].reason, "no_headroom");
    // The whole AC requirement of the pack is reported as missing, not a partial figure.
    let expected_missing = 1.92 * (1.0 - 0.13) / 0.95;
    let unassigned = &result.unassigned[0];
    assert!(
        (unassigned.missing_kwh - expected_missing).abs() < 0.01,
        "missing_kwh {} should be the pack's full AC need {expected_missing}",
        unassigned.missing_kwh
    );
    assert_eq!(unassigned.missing_kwh, unassigned.required_kwh);
    assert_eq!(unassigned.available_kwh, 0.0);
}

#[tokio::test]
async fn a_restart_aborts_an_unobserved_dwell_without_stamping_history() {
    let ledger = fakes::ledger_with(&["ONE"]);
    ledger
        .packs()
        .unwrap()
        .begin_dwell("ONE", NOW - 600)
        .unwrap();
    let history: Arc<dyn FullChargeHistory> =
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = Arc::new(MaintenancePort::new(Arc::clone(&history), settings()));
    let harness = harness::build_on(
        Arc::clone(&ledger),
        Authority::Auto,
        vec![pack("ONE", 100.0)],
        plan(NOW, 0.0, 1.92),
    );
    harness.source.set_maintenance(Arc::clone(&port));

    harness.engine.boot(NOW).await.expect("boot");

    assert_eq!(history.dwell_started_at("ONE"), None);
    assert_eq!(history.last_full_at("ONE"), None);
    let events = harness.ledger.events().unwrap().recent(20).unwrap();
    assert!(events.iter().any(|event| {
        event.kind == "maintenance.dwell_failed" && event.message.contains("restart_unobserved")
    }));
}

#[tokio::test]
async fn an_unassigned_pack_is_exposed_in_the_snapshot() {
    let ledger = fakes::ledger_with(&["ONE"]);
    let port = Arc::new(MaintenancePort::new(
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger))),
        settings(),
    ));
    let mut source = plan(NOW, 0.0, 1.92);
    source.maintenance_unassigned.push(UnassignedPack {
        pack_id: "ONE".into(),
        reason: "insufficient_capacity",
        required_kwh: 1.8,
        available_kwh: 1.4,
        missing_kwh: 0.42,
        deadline: NOW + 3_600,
    });
    let harness = harness::build(Authority::DryRun, vec![pack("ONE", 13.0)], source);
    harness.source.set_maintenance(port);
    harness.engine.tick_once(NOW).await.expect("tick");

    let snapshot = harness.engine.snapshot(
        NOW,
        &FleetView::new(vec![pack("ONE", 13.0)]),
        &MeterReading::default(),
    );
    let maintenance = snapshot.packs[0].maintenance.as_ref().expect("maintenance");
    assert_eq!(maintenance.state, "unassigned_insufficient_capacity");
    assert_eq!(maintenance.unassigned_reason, Some("insufficient_capacity"));
    assert_eq!(maintenance.missing_kwh, Some(0.42));
}
