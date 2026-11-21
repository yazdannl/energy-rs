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
    ChargeCandidate, ChargeSlot, DuePack, FullChargeAssignment, FullChargeSettings,
};
use energy_core::dispatch::FleetView;
use energy_core::maintenance::{
    FullChargeHistory, LedgerFullChargeHistory, MaintenancePort, apply, candidates, progress,
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
    assert_eq!(assignments.assignments.len(), 1);
    assert!(assignments.unassigned.is_empty());
    let assignment = &assignments.assignments[0];
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
    assert!(assignments.assignments.is_empty());
}

#[test]
fn a_short_cheap_window_books_what_it_can_and_reports_the_rest() {
    // Five cheap slots at 800 W deliver 1 kWh; the nearly empty pack needs 1.92 kWh AC. Book the
    // whole band as a partial charge instead of dropping the pack, and report the 0.92 kWh short.
    let candidates = slots(&prices(5, 40), 800.0);
    let assignments = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &[due_pack("ONE", 5.0, None, false)],
        7_300.0,
    );
    assert_eq!(assignments.assignments.len(), 1);
    assert!(assignments.unassigned.is_empty());
    let partial = &assignments.assignments[0];
    assert!(!partial.is_complete());
    assert_eq!(partial.partial_reason, Some("not_enough_eligible_slots"));
    assert!((partial.missing_kwh - 0.92).abs() < 0.01);
    let charged: f64 = partial
        .slots
        .iter()
        .map(|slot| slot.charge_w * slot.duration_s as f64 / 3_600_000.0)
        .sum();
    assert!((charged - 1.0).abs() < 1e-6);
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
    assert_eq!(assignments.assignments.len(), 4);
    let mut used = std::collections::BTreeMap::new();
    for assignment in &assignments.assignments {
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
    let mut spec = pack("ONE", 90.0).spec;
    spec.max_soc_pct = 100.0;
    let pack = PackState {
        spec,
        soc_pct: 90.0,
    };
    let assignments = port.assignments(NOW, &[pack], &slots(&prices(12, 12), 800.0), 7_300.0);
    assert_eq!(assignments.assignments.len(), 1);
    assert_eq!(assignments.assignments[0].reason, "manual_request");
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
    for slot in &mut plan.slots {
        slot.battery_kw = 0.0;
    }
    for (index, slot) in plan.slots.iter_mut().enumerate() {
        slot.buy_all_in = if index < 20 { 0.08 } else { 0.30 };
        slot.load_kw = 0.5;
        slot.pv_kw = 0.0;
    }
    let candidates = candidates(&plan, NOW, 7_300.0);
    let mut state = pack("ONE", 5.0).state().unwrap();
    state.spec.max_soc_pct = 100.0;
    let assignments = port.assignments(NOW, &[state], &candidates, 7_300.0);
    assert_eq!(assignments.assignments.len(), 1);
    assert!(
        assignments.assignments[0]
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

#[test]
fn a_short_window_reports_the_missing_kwh_on_the_partial_charge() {
    let result = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &slots(&prices(5, 40), 800.0),
        &[due_pack("ONE", 5.0, None, false)],
        7_300.0,
    );
    assert_eq!(result.assignments.len(), 1);
    assert!(result.unassigned.is_empty());
    let pack = &result.assignments[0];
    assert!(!pack.is_complete());
    assert_eq!(pack.partial_reason, Some("not_enough_eligible_slots"));
    assert!(pack.missing_kwh > 0.9 && pack.missing_kwh < 1.0);
    let charged: f64 = pack
        .slots
        .iter()
        .map(|slot| slot.charge_w * slot.duration_s as f64 / 3_600_000.0)
        .sum();
    assert!((charged + pack.missing_kwh - 1.92).abs() < 1e-6);
}

#[test]
fn the_earliest_eligible_window_wins_over_a_cheaper_later_one() {
    // Both cheap windows are inside the band; a long expensive tail keeps the mean high.
    let prices: Vec<f64> = [0.08; 12]
        .into_iter()
        .chain([0.05; 12])
        .chain([0.30; 60])
        .collect();
    let candidates = slots(&prices, 800.0);
    let start = candidates[12].t_start;
    let result = energy_battery::protection::full_charge::interval::plan(
        NOW,
        &settings(),
        &candidates,
        &[due_pack("ONE", 5.0, None, false)],
        7_300.0,
    );
    assert_eq!(result.assignments.len(), 1);
    assert!(
        result.assignments[0]
            .slots
            .iter()
            .all(|slot| slot.t_start < start)
    );
}

#[test]
fn a_replan_two_minutes_before_a_boundary_uses_only_the_remaining_seconds() {
    let start = NOW - NOW.rem_euclid(SLOT_SECONDS);
    let mut plan = plan(start, 0.0, 0.1);
    // The first hour must be in the band, or the clipped two minutes are the only eligible
    // slot and no pack can be completed from them.
    for slot in plan.slots.iter_mut().take(16) {
        slot.buy_all_in = 0.08;
    }
    plan.slots[0].load_kw = 0.0;
    plan.slots[0].pv_kw = 0.0;
    let late = start + SLOT_SECONDS - 120;
    let candidates = candidates(&plan, late, 7_300.0);
    assert_eq!(candidates[0].t_start, late);
    assert_eq!(candidates[0].t_end - late, 120);
    let result = energy_battery::protection::full_charge::interval::plan(
        late,
        &settings(),
        &candidates,
        &[due_pack("ONE", 5.0, None, false)],
        7_300.0,
    );
    assert_eq!(result.assignments.len(), 1);
    let first = result.assignments[0]
        .slots
        .iter()
        .find(|slot| slot.slot_start == start)
        .unwrap();
    assert_eq!(first.duration_s, 120);
    assert!(first.charge_w <= 800.0);
}

#[test]
fn the_fold_replaces_a_conflicting_discharge_and_scales_a_clipped_slot() {
    let mut plan = plan(NOW, 0.0, 1.92);
    plan.slots[0].battery_kw = -0.8;
    let charge = ChargeSlot {
        slot_start: plan.slots[0].t_start,
        t_start: plan.slots[0].t_start + 780,
        t_end: plan.slots[0].t_start + SLOT_SECONDS,
        duration_s: 120,
        price_eur_per_kwh: 0.08,
        charge_w: 800.0,
    };
    apply(
        &mut plan,
        &[FullChargeAssignment {
            pack_id: "ONE".into(),
            reason: "interval_due",
            target_soc_pct: 100.0,
            slots: vec![charge],
            charge_end: NOW + 120,
            dwell_until: NOW + 3_720,
            expected_cost_eur: 0.003,
            missing_kwh: 0.0,
            partial_reason: None,
            band_deadline: NOW,
            band_ceil_eur_per_kwh: 0.08,
        }],
        1.92,
        0.95,
    );
    assert!((plan.slots[0].battery_kw - 800.0 * 120.0 / SLOT_SECONDS as f64 / 1000.0).abs() < 1e-9);
}

#[test]
fn a_stale_dwell_is_aborted_when_its_assignment_disappears() {
    let ledger = fakes::ledger_with(&["ONE"]);
    ledger.packs().unwrap().begin_dwell("ONE", NOW).unwrap();
    let history = Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = Arc::new(MaintenancePort::new(history.clone(), settings()));
    let plan = plan(NOW, 0.0, 1.92);
    let outcomes = progress(&port, &plan, &FleetView::new(vec![pack("ONE", 100.0)]), NOW);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].reason, "assignment_lost");
    assert_eq!(history.dwell_started_at("ONE"), None);
    assert_eq!(history.last_full_at("ONE"), None);
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
        missing_kwh: 0.0,
        partial_reason: None,
        band_deadline: charge_end,
        band_ceil_eur_per_kwh: 0.08,
    }
}
