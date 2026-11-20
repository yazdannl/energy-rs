//! The seven-day, price-aware full-charge rule: slots, dwell and safety.
//!
//! These are planner and wiring tests; no Home Assistant or device is involved.

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
    ChargeCandidate, DuePack, FullChargeAssignment, FullChargeSettings,
};
use energy_core::dispatch::FleetView;
use energy_core::maintenance::{
    FullChargeHistory, LedgerFullChargeHistory, MaintenancePort, candidates, progress,
};
use energy_core::plan::{PackState, SLOT_SECONDS};
use energy_core::safety::Authority;
use energy_core::stream::EngineCommand;
use energy_ledger::EventLevel;
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
            t_start: NOW + index as i64 * SLOT_SECONDS,
            t_end: NOW + (index as i64 + 1) * SLOT_SECONDS,
            price_eur_per_kwh: *price,
            available_w,
        })
        .collect()
}

fn prices(cheap: usize, expensive: usize) -> Vec<f64> {
    (0..cheap)
        .map(|_| 0.08)
        .chain((0..expensive).map(|_| 0.30))
        .collect()
}

#[test]
fn a_due_pack_gets_the_cheapest_slots_that_cover_its_missing_energy() {
    // 5 % -> 100 % needs 1.92 kWh AC: 10 slots at 800 W, plus rounding. The 12 cheapest slots
    // are at 0.08 EUR/kWh while the rest of the horizon is 0.30.
    let candidates = slots(&prices(12, 30), 800.0);
    let assignments = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &[due_pack("ONE", 5.0, Some(NOW - 8 * 86_400), false)],
        7_300.0,
    );
    assert_eq!(assignments.len(), 1);
    let assignment = &assignments[0];
    assert_eq!(assignment.reason, "interval_due");
    assert_eq!(assignment.target_soc_pct, 100.0);
    assert!(
        assignment
            .slots
            .iter()
            .all(|slot| slot.price_eur_per_kwh == 0.08)
    );
    assert!((assignment.slots.len() as f64 * 0.2) >= 1.92);
    assert_eq!(assignment.dwell_until - assignment.charge_end, 3_600);
    assert!(assignment.expected_cost_eur > 0.0);
}

#[test]
fn a_pack_inside_the_interval_is_not_planned() {
    let candidates = slots(&prices(24, 24), 800.0);
    let assignments = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &[due_pack("ONE", 40.0, Some(NOW - 2 * 86_400), false)],
        7_300.0,
    );
    assert!(assignments.is_empty());
}

#[test]
fn a_short_cheap_window_waits_for_the_next_replan() {
    // Only five cheap slots (1 kWh) cannot cover the 2 kWh a nearly empty pack needs: no partial
    // maintenance charge, and the pack keeps its cheap chance on the next plan.
    let candidates = slots(&prices(5, 40), 800.0);
    let assignments = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &[due_pack("ONE", 5.0, None, false)],
        7_300.0,
    );
    assert!(assignments.is_empty());
}

#[test]
fn four_packs_fit_under_the_import_ceiling_by_sharing_slots() {
    let mut packs = Vec::new();
    for index in 0..4 {
        packs.push(due_pack(
            &format!("PACK-{index}"),
            5.0,
            Some(NOW - 10 * 86_400),
            false,
        ));
    }
    let candidates = slots(&prices(60, 60), 2_000.0);
    let assignments = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &packs,
        7_300.0,
    );
    assert_eq!(assignments.len(), 4);
    let mut used = std::collections::BTreeMap::new();
    for assignment in &assignments {
        for slot in &assignment.slots {
            let entry = used.entry(slot.t_start).or_insert(0.0);
            *entry += slot.charge_w;
            assert!(
                *entry <= 2_000.0 + 1e-6,
                "slot {} exceeds the budget",
                slot.t_start
            );
        }
    }
}

#[test]
fn a_manual_request_forces_the_rule_early_without_changing_history() {
    let ledger = fakes::ledger_with(&["ONE"]);
    let history = Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = Arc::new(MaintenancePort::new(history, settings()));
    port.force(Some("ONE"));
    let pack = PackState {
        spec: pack("ONE", 90.0).spec,
        soc_pct: 90.0,
    };
    let assignments = port.assignments(NOW, &[pack], &slots(&prices(12, 12), 800.0), 7_300.0);
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].reason, "manual_request");
}

#[test]
fn the_dwell_stamps_last_full_only_when_it_completes() {
    let ledger = fakes::ledger_with(&["ONE"]);
    let history = Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = Arc::new(MaintenancePort::new(history.clone(), settings()));
    let mut plan = plan(NOW, 0.0, 1.92);
    plan.maintenance
        .push(assignment("ONE", NOW - 900, NOW + 3_600));
    let view = FleetView::new(vec![pack("ONE", 100.0)]);

    let first = progress(&port, &plan, &view, NOW);
    assert!(
        first.is_empty(),
        "passing 99 % starts the dwell but does not stamp it"
    );
    assert_eq!(history.last_full_at("ONE"), None);
    assert_eq!(history.dwell_started_at("ONE"), Some(NOW));

    let later = progress(&port, &plan, &view, NOW + 3_600);
    assert_eq!(later.len(), 1);
    assert!(later[0].completed);
    assert_eq!(history.last_full_at("ONE"), Some(NOW + 3_600));
    assert_eq!(history.dwell_started_at("ONE"), None);
}

#[test]
fn a_failed_dwell_never_stamps_last_full() {
    let ledger = fakes::ledger_with(&["ONE"]);
    let history = Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = Arc::new(MaintenancePort::new(history.clone(), settings()));
    let mut plan = plan(NOW, 0.0, 1.92);
    plan.maintenance
        .push(assignment("ONE", NOW - 900, NOW + 3_600));
    let view = FleetView::new(vec![pack("ONE", 82.0)]);

    let outcomes = progress(&port, &plan, &view, NOW + 100);
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].completed);
    assert_eq!(outcomes[0].reason, "target_lost");
    assert_eq!(history.last_full_at("ONE"), None);
    assert_eq!(history.dwell_started_at("ONE"), None);
}

#[test]
fn a_due_pack_gets_its_charge_from_the_solved_plan() {
    let ledger = fakes::ledger_with(&["ONE"]);
    ledger
        .packs()
        .unwrap()
        .upsert(&energy_ledger::Pack {
            last_full_charge_at: Some(NOW - 10 * 86_400),
            ..energy_ledger::Pack::new("ONE", 1.92, NOW)
        })
        .unwrap();
    let port = Arc::new(MaintenancePort::new(
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger))),
        settings(),
    ));
    let mut plan = plan(NOW, 0.0, 0.1);
    for (index, slot) in plan.slots.iter_mut().enumerate() {
        slot.buy_all_in = if index < 20 { 0.08 } else { 0.30 };
        slot.load_kw = 0.5;
        slot.pv_kw = 0.0;
    }
    let candidates = candidates(&plan, NOW, 7_300.0);
    let assignments = port.assignments(
        NOW,
        &[pack("ONE", 5.0).state().unwrap()],
        &candidates,
        7_300.0,
    );
    assert_eq!(assignments.len(), 1);
    assert!(
        assignments[0]
            .slots
            .iter()
            .all(|slot| slot.price_eur_per_kwh == 0.08)
    );
}

#[tokio::test]
async fn a_dry_run_journals_the_intent_without_writing() {
    let ledger = fakes::ledger_with(&["ONE"]);
    let port = Arc::new(MaintenancePort::new(
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger))),
        settings(),
    ));
    let mut plan = plan(NOW, 0.0, 0.1);
    plan.maintenance.push(assignment("ONE", NOW, NOW + 3_600));
    let harness = harness::build(Authority::DryRun, vec![pack("ONE", 40.0)], plan);
    harness.source.set_maintenance(Arc::clone(&port));

    harness
        .engine
        .apply_command(
            EngineCommand::FullCharge {
                pack: Some("ONE".into()),
            },
            NOW,
        )
        .await
        .unwrap();

    assert!(harness.sink.writes().is_empty());
    let events = harness.ledger.events().unwrap().recent(20).unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.kind == "maintenance.requested")
    );
    assert!(
        events
            .iter()
            .any(|event| event.kind == "maintenance.planned" && event.level == EventLevel::Info)
    );
    assert_eq!(port.history().last_full_at("ONE"), None);
}

fn assignment(pack_id: &str, charge_end: i64, dwell_until: i64) -> FullChargeAssignment {
    FullChargeAssignment {
        pack_id: pack_id.into(),
        reason: "interval_due",
        target_soc_pct: 100.0,
        slots: Vec::new(),
        charge_end,
        dwell_until,
        expected_cost_eur: 0.0,
    }
}
