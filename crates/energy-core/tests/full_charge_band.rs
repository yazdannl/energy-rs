//! Cheap-band stability: replans, deadline rollovers, price revisions and manual requests.

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
    ChargeCandidate, ChargeSlot, DuePack, FullChargeAssignment, FullChargeSettings, interval,
};
use energy_core::maintenance::{FullChargeHistory, LedgerFullChargeHistory, MaintenancePort};
use energy_core::plan::SLOT_SECONDS;
use support::fakes::{self, NOW, pack};

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

/// Ten-minute slots whose shrinking-window mean would drop below the 0.342 band after one slot.
fn drifting_band(start: i64, from: usize) -> Vec<ChargeCandidate> {
    (from..30)
        .map(|index| {
            let at = start + index as i64 * 600;
            ChargeCandidate {
                slot_start: at,
                t_start: at,
                t_end: at + 600,
                price_eur_per_kwh: if index < 8 {
                    0.40
                } else if index < 20 {
                    0.342
                } else {
                    0.30
                },
                available_w: 800.0,
            }
        })
        .collect()
}

fn revised_band(start: i64, from: usize) -> Vec<ChargeCandidate> {
    drifting_band(start, from)
        .into_iter()
        .map(|mut slot| {
            if slot.slot_start == start + 8 * 600 {
                slot.price_eur_per_kwh = 0.20;
            }
            slot
        })
        .collect()
}

fn clipped_band(start: i64, elapsed: i64) -> Vec<ChargeCandidate> {
    (0..5)
        .map(|index| ChargeCandidate {
            slot_start: start + index * SLOT_SECONDS,
            t_start: start + index * SLOT_SECONDS + if index == 0 { elapsed } else { 0 },
            t_end: start + (index + 1) * SLOT_SECONDS,
            price_eur_per_kwh: 0.08,
            available_w: 1_000.0,
        })
        .collect()
}

fn starts(assignment: &FullChargeAssignment) -> Vec<i64> {
    assignment
        .slots
        .iter()
        .map(|slot| slot.slot_start)
        .collect()
}

fn charged_kwh(slots: &[ChargeSlot]) -> f64 {
    slots
        .iter()
        .map(|slot| slot.charge_w * slot.duration_s as f64 / 3_600_000.0)
        .sum()
}

fn stamp_full(ledger: &Arc<energy_ledger::Ledger>, pack_id: &str) {
    ledger
        .packs()
        .expect("packs")
        .finish_dwell(pack_id, NOW)
        .expect("stamp");
}

#[test]
fn a_replan_ten_minutes_later_keeps_the_same_contiguous_charge() {
    let start = NOW - NOW.rem_euclid(600);
    let first = interval::plan(
        start,
        &settings(),
        &drifting_band(start, 0),
        &[due_pack("ONE")],
        7_500.0,
    );
    let second = interval::plan_after(
        start + 600,
        &settings(),
        &drifting_band(start, 1),
        &[due_pack("ONE")],
        7_500.0,
        &first.assignments,
    );
    let before = starts(&first.assignments[0]);
    let after = starts(&second.assignments[0]);
    assert_eq!(after[0], before[0], "the charge moved to a later slot");
    for planned in [&before, &after] {
        assert!(planned.windows(2).all(|pair| pair[1] - pair[0] == 600));
    }
    assert!(second.assignments[0].is_complete());
}

#[test]
fn a_band_that_would_drop_a_booked_slot_does_not_evict_it() {
    let start = NOW - NOW.rem_euclid(600);
    let first = interval::plan(
        start,
        &settings(),
        &drifting_band(start, 0),
        &[due_pack("ONE")],
        7_500.0,
    );
    let anchored_start = starts(&first.assignments[0])[0];
    let later = start + 600;
    let unanchored = interval::plan(
        later,
        &settings(),
        &drifting_band(start, 1),
        &[due_pack("ONE")],
        7_500.0,
    );
    assert!(
        starts(&unanchored.assignments[0])[0] > anchored_start,
        "the drifting threshold should have excluded the booked slot"
    );
    let anchored = interval::plan_after(
        later,
        &settings(),
        &drifting_band(start, 1),
        &[due_pack("ONE")],
        7_500.0,
        &first.assignments,
    );
    assert_eq!(starts(&anchored.assignments[0])[0], anchored_start);
}

#[test]
fn the_shared_port_keeps_the_band_when_the_new_plan_has_no_maintenance_rows() {
    let start = NOW - NOW.rem_euclid(600);
    let ledger = fakes::ledger_with(&["ONE"]);
    let history: Arc<dyn FullChargeHistory> = Arc::new(LedgerFullChargeHistory::new(ledger));
    let port = MaintenancePort::new(history, settings());
    let mut state = pack("ONE", 13.0).state().expect("pack state");
    state.spec.max_soc_pct = 100.0;
    let first = port.assignments(start, &[state.clone()], &drifting_band(start, 0), 7_500.0);
    let anchored = starts(&first.assignments[0])[0];
    let second = port.assignments(start + 600, &[state], &drifting_band(start, 1), 7_500.0);
    assert_eq!(starts(&second.assignments[0])[0], anchored);
}

#[test]
fn the_band_anchor_is_dropped_at_the_next_overnight_deadline() {
    let start = NOW - NOW.rem_euclid(600);
    let first = interval::plan(
        start,
        &settings(),
        &drifting_band(start, 0),
        &[due_pack("ONE")],
        7_500.0,
    );
    let next_night = start + 25 * 3_600;
    let at = next_night + 600;
    let plain = interval::plan(
        at,
        &settings(),
        &drifting_band(next_night, 1),
        &[due_pack("ONE")],
        7_500.0,
    );
    let anchored = interval::plan_after(
        at,
        &settings(),
        &drifting_band(next_night, 1),
        &[due_pack("ONE")],
        7_500.0,
        &first.assignments,
    );
    assert_ne!(
        first.assignments[0].band_deadline,
        anchored.assignments[0].band_deadline
    );
    assert_eq!(
        starts(&anchored.assignments[0]),
        starts(&plain.assignments[0])
    );
}

#[test]
fn a_downward_price_revision_drops_the_band_anchor() {
    let start = NOW - NOW.rem_euclid(600);
    let first = interval::plan(
        start,
        &settings(),
        &drifting_band(start, 0),
        &[due_pack("ONE")],
        7_500.0,
    );
    assert_eq!(starts(&first.assignments[0])[0], start + 8 * 600);
    let at = start + 600;
    let plain = interval::plan(
        at,
        &settings(),
        &revised_band(start, 1),
        &[due_pack("ONE")],
        7_500.0,
    );
    let anchored = interval::plan_after(
        at,
        &settings(),
        &revised_band(start, 1),
        &[due_pack("ONE")],
        7_500.0,
        &first.assignments,
    );
    assert_eq!(
        starts(&anchored.assignments[0]),
        starts(&plain.assignments[0])
    );
    assert!(starts(&plain.assignments[0])[0] > start + 8 * 600);
}

#[test]
fn a_manual_request_for_a_refused_pack_survives_another_packs_completion() {
    let ledger = fakes::ledger_with(&["A", "B"]);
    let history: Arc<dyn FullChargeHistory> =
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = MaintenancePort::new(history, settings());
    port.force(Some("A"));
    port.force(Some("B"));
    let candidates = slot_book(&[0.5; 20]);
    let mut a = pack("A", 13.0).state().expect("A");
    a.spec.max_soc_pct = 95.0;
    let mut b = pack("B", 13.0).state().expect("B");
    b.spec.max_soc_pct = 100.0;
    let first = port.assignments(NOW, &[a.clone(), b.clone()], &candidates, 7_500.0);
    assert_eq!(first.assignments[0].pack_id, "B");
    assert_eq!(first.unassigned[0].pack_id, "A");
    stamp_full(&ledger, "B");
    a.spec.max_soc_pct = 100.0;
    let second = port.assignments(NOW + 60, &[a, b], &candidates, 7_500.0);
    assert_eq!(second.assignments.len(), 1);
    assert_eq!(second.assignments[0].pack_id, "A");
    assert_eq!(second.assignments[0].reason, "manual_request");
}

#[test]
fn a_fleet_wide_request_is_narrowed_to_its_unresolved_members() {
    let ledger = fakes::ledger_with(&["A", "B"]);
    let history: Arc<dyn FullChargeHistory> =
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(&ledger)));
    let port = MaintenancePort::new(history, settings());
    port.force(None);
    let candidates = slot_book(&[0.5; 20]);
    let mut a = pack("A", 13.0).state().expect("A");
    a.spec.max_soc_pct = 95.0;
    let mut b = pack("B", 13.0).state().expect("B");
    b.spec.max_soc_pct = 100.0;
    let first = port.assignments(NOW, &[a.clone(), b.clone()], &candidates, 7_500.0);
    assert_eq!(first.assignments[0].pack_id, "B");
    stamp_full(&ledger, "B");
    a.spec.max_soc_pct = 100.0;
    let second = port.assignments(NOW + 60, &[a, b], &candidates, 7_500.0);
    assert_eq!(second.assignments.len(), 1);
    assert_eq!(second.assignments[0].pack_id, "A");
}

#[test]
fn missing_kwh_does_not_grow_when_only_time_inside_a_planned_slot_passes() {
    let start = NOW - NOW.rem_euclid(SLOT_SECONDS);
    let first = interval::plan(
        start + 780,
        &settings(),
        &clipped_band(start, 780),
        &[due_pack("ONE")],
        7_500.0,
    );
    let later = interval::plan(
        start + 840,
        &settings(),
        &clipped_band(start, 840),
        &[due_pack("ONE")],
        7_500.0,
    );
    assert_eq!(first.assignments.len(), 1);
    assert_eq!(later.assignments.len(), 1);
    let early = &first.assignments[0];
    let late = &later.assignments[0];
    assert!(!early.is_complete() && !late.is_complete());
    assert_eq!(early.missing_kwh, late.missing_kwh);
    assert!((early.missing_kwh - (REQUIRED_KWH - 1.0)).abs() < 1e-6);
    assert!(charged_kwh(&early.slots) > charged_kwh(&late.slots));
}

fn slot_book(prices: &[f64]) -> Vec<ChargeCandidate> {
    prices
        .iter()
        .enumerate()
        .map(|(index, price)| ChargeCandidate {
            slot_start: NOW + index as i64 * SLOT_SECONDS,
            t_start: NOW + index as i64 * SLOT_SECONDS,
            t_end: NOW + (index as i64 + 1) * SLOT_SECONDS,
            price_eur_per_kwh: *price,
            available_w: 800.0,
        })
        .collect()
}
