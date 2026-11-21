//! Partial maintenance charges: book what the cheap band can deliver, without claiming success.

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

use std::collections::BTreeMap;
use std::sync::Arc;

use energy_battery::protection::full_charge::{
    ChargeCandidate, ChargeSlot, DuePack, FullChargeAssignment, FullChargeSettings, IntervalPlan,
    interval,
};
use energy_core::dispatch::FleetView;
use energy_core::maintenance::{
    FullChargeHistory, LedgerFullChargeHistory, MaintenancePort, apply, candidates, progress,
};
use energy_core::plan::SLOT_SECONDS;
use energy_core::stream::MaintenanceSnapshot;
use support::fakes::{self, NOW, pack, plan};

const REQUIRED_KWH: f64 = 1.92 * (100.0 - 13.0) / 100.0 / 0.95;

fn settings() -> FullChargeSettings {
    FullChargeSettings {
        enabled: true,
        interval_days: 7,
        dwell_minutes: 60,
        ..FullChargeSettings::weekly(800)
    }
}

fn due_pack(id: &str) -> DuePack {
    DuePack {
        pack_id: id.into(),
        capacity_kwh: 1.92,
        soc_pct: 13.0,
        max_soc_pct: 100.0,
        eta_charge: 0.95,
        max_charge_w: 800.0,
        last_full_at: None,
        forced: false,
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

fn charged_kwh(slots: &[ChargeSlot]) -> f64 {
    slots
        .iter()
        .map(|slot| slot.charge_w * slot.duration_s as f64 / 3_600_000.0)
        .sum()
}

fn starts(assignment: &FullChargeAssignment) -> Vec<i64> {
    assignment
        .slots
        .iter()
        .map(|slot| slot.slot_start)
        .collect()
}

fn assert_slot_budget(result: &IntervalPlan, cap_w: f64) {
    let mut used = BTreeMap::new();
    for assignment in &result.assignments {
        for slot in &assignment.slots {
            assert!(slot.charge_w <= 800.0 + 1e-6);
            let entry = used.entry(slot.slot_start).or_insert(0.0);
            *entry += slot.charge_w;
            assert!(*entry <= cap_w + 1e-6, "slot {} over budget", slot.t_start);
        }
    }
}

fn assert_partial_has_no_dwell(result: &IntervalPlan, pack_id: &str) {
    let ledger = fakes::ledger_with(&["PACK-0", "PACK-1", "PACK-2", "PACK-3"]);
    let history: Arc<dyn FullChargeHistory> =
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = Arc::new(MaintenancePort::new(history.clone(), settings()));
    let mut source = plan(NOW, 0.0, 1.92);
    source.maintenance.clone_from(&result.assignments);
    let end = result
        .assignments
        .iter()
        .find(|entry| entry.pack_id == pack_id)
        .map_or(NOW, |entry| entry.charge_end);
    let view = FleetView::new((0..4).map(|i| pack(&format!("PACK-{i}"), 100.0)).collect());
    progress(&port, &source, &view, end);
    progress(&port, &source, &view, end + 3_600);
    assert_eq!(history.dwell_started_at(pack_id), None);
    assert_eq!(history.last_full_at(pack_id), None);
}

fn assert_four_pack_result(result: &IntervalPlan) {
    assert_eq!(result.assignments.len(), 4, "{:?}", result.unassigned);
    assert!(result.unassigned.is_empty());
    for assignment in result.assignments.iter().take(3) {
        assert!(assignment.is_complete());
        assert_eq!(assignment.missing_kwh, 0.0);
        assert_eq!(assignment.partial_reason, None);
    }
    let partial = &result.assignments[3];
    assert_eq!(partial.pack_id, "PACK-3");
    assert!(!partial.is_complete());
    assert_eq!(partial.partial_reason, Some("not_enough_eligible_slots"));
    let booked = charged_kwh(&partial.slots);
    assert!(booked > 1.2 && booked < 1.5, "booked {booked} kWh");
    assert!(partial.missing_kwh > 0.2 && partial.missing_kwh < 0.4);
    let snapshot = MaintenanceSnapshot::assigned(partial, NOW);
    assert_eq!(snapshot.state, "charging_partial");
    assert_eq!(
        snapshot.unassigned_reason,
        Some("not_enough_eligible_slots")
    );
    assert_eq!(snapshot.missing_kwh, Some(partial.missing_kwh));
}

#[test]
fn four_due_packs_get_a_partial_fourth_without_a_dwell_or_history_stamp() {
    let prices: Vec<f64> = [0.08; 12].into_iter().chain([0.30; 40]).collect();
    let candidates = slots(&prices, 2_700.0);
    let packs: Vec<DuePack> = (0..4)
        .map(|index| due_pack(&format!("PACK-{index}")))
        .collect();
    let result = interval::plan(NOW, &settings(), &candidates, &packs, 7_500.0);
    assert_four_pack_result(&result);
    assert_slot_budget(&result, 2_700.0);
    assert_partial_has_no_dwell(&result, "PACK-3");
}

#[test]
fn a_pack_with_no_bookable_slot_stays_unassigned_with_its_cause() {
    let result = interval::plan(
        NOW,
        &settings(),
        &slots(&[0.5; 12], 0.0),
        &[due_pack("NO-HEADROOM")],
        7_500.0,
    );
    assert!(result.assignments.is_empty());
    let refused = &result.unassigned[0];
    assert_eq!(refused.reason, "no_headroom");
    assert_eq!(refused.available_kwh, 0.0);
    assert!((refused.missing_kwh - REQUIRED_KWH).abs() < 1e-6);

    let mut capped = due_pack("CAPPED");
    capped.max_soc_pct = 95.0;
    let result = interval::plan(
        NOW,
        &settings(),
        &slots(&[0.08; 12], 2_000.0),
        &[capped],
        7_500.0,
    );
    assert!(result.assignments.is_empty());
    assert_eq!(result.unassigned[0].reason, "soc_ceiling_below_target");
    assert!((result.unassigned[0].missing_kwh - REQUIRED_KWH).abs() < 1e-6);
}

#[test]
fn a_partial_charge_never_pushes_a_spiked_slot_over_the_import_budget() {
    let mut source = plan(NOW, 0.0, 1.92);
    for (index, slot) in source.slots.iter_mut().enumerate() {
        slot.buy_all_in = if index < 12 { 0.08 } else { 0.30 };
        slot.load_kw = if index == 6 { 5.0 } else { 4.8 };
        slot.pv_kw = 0.0;
        slot.battery_kw = 0.0;
    }
    let candidates = candidates(&source, NOW, 7_500.0);
    let packs: Vec<DuePack> = (0..4)
        .map(|index| due_pack(&format!("PACK-{index}")))
        .collect();
    let result = interval::plan(NOW, &settings(), &candidates, &packs, 7_500.0);
    assert_eq!(result.assignments.len(), 4);
    assert!(result.assignments.iter().any(|entry| !entry.is_complete()));
    let mut used = BTreeMap::new();
    for assignment in &result.assignments {
        for slot in &assignment.slots {
            let entry = used.entry(slot.slot_start).or_insert(0.0);
            *entry += slot.charge_w;
        }
    }
    for candidate in candidates.iter().take(12) {
        let reserved = used.get(&candidate.slot_start).copied().unwrap_or(0.0);
        assert!(
            reserved <= candidate.available_w + 1e-6,
            "slot {} reserves {} W of {} W",
            candidate.slot_start,
            reserved,
            candidate.available_w
        );
    }
    apply(&mut source, &result.assignments, 1.92 * 4.0, 0.95);
    for slot in &source.slots {
        let grid_w = slot.load_kw * 1000.0 + slot.battery_kw * 1000.0;
        assert!(grid_w <= 7_500.0 + 1e-6, "folded import {grid_w} W");
    }
}

fn islands(start: i64) -> Vec<ChargeCandidate> {
    (0..16)
        .map(|index| {
            let at = start + i64::from(index) * 600;
            ChargeCandidate {
                slot_start: at,
                t_start: at,
                t_end: at + 600,
                price_eur_per_kwh: if (4..8).contains(&index) { 0.9 } else { 0.5 },
                available_w: 800.0,
            }
        })
        .collect()
}

fn six_slots(available_w: f64) -> Vec<ChargeCandidate> {
    let start = NOW - NOW.rem_euclid(600);
    (0..6)
        .map(|index| {
            let at = start + i64::from(index) * 600;
            ChargeCandidate {
                slot_start: at,
                t_start: at,
                t_end: at + 600,
                price_eur_per_kwh: 0.5,
                available_w,
            }
        })
        .collect()
}

#[test]
fn a_charge_never_joins_two_cheap_islands_across_an_expensive_gap() {
    let start = NOW - NOW.rem_euclid(600);
    let result = interval::plan(
        start,
        &settings(),
        &islands(start),
        &[due_pack("ONE")],
        7_500.0,
    );
    let assignment = &result.assignments[0];
    assert!(!assignment.is_complete());
    let booked = starts(assignment);
    assert!(booked.iter().all(|slot| *slot >= start + 8 * 600));
    assert!(booked.windows(2).all(|pair| pair[1] - pair[0] == 600));
}

#[test]
fn an_earlier_packs_partial_fill_shortens_the_run_left_to_a_later_pack() {
    let result = interval::plan(
        NOW,
        &settings(),
        &six_slots(1_200.0),
        &[due_pack("A_SMALL"), {
            let mut big = due_pack("B_BIG");
            big.max_charge_w = 2_000.0;
            big
        }],
        7_500.0,
    );
    assert_eq!(result.assignments.len(), 2);
    let small = &result.assignments[0];
    let big = &result.assignments[1];
    assert!(!small.is_complete() && !big.is_complete());
    assert!(small.slots.iter().all(|slot| slot.charge_w == 800.0));
    assert!(big.slots.iter().all(|slot| slot.charge_w == 400.0));
    assert!(big.missing_kwh > small.missing_kwh);
}
