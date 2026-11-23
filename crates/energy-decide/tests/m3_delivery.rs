//! M3 scenarios S23/S152 and S161: delivery axis and generated physical invariants.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::plan_assemble::{
    DeliveryOutcome, DispatchPermission, Obligation, ObligationClass, PackPower, PlanState,
    assemble_plan, classify_delivery,
};
use support::{assembled, bare_horizon, flat, plan_input, prices, settlement, ts};

#[test]
fn s23_one_absent_pack_is_partial_delivery_not_a_fleet_infeasibility() {
    let (plan, delivery) = dropout_replay();
    assert_eq!(delivery.outcome, DeliveryOutcome::Partial);
    assert_eq!(delivery.failed_pack_ids, vec!["3".to_owned()]);
    assert_eq!(delivery.applied_w, -800.0);
    assert_eq!(delivery.shortfall_w, 2400.0);
    let orphan = delivery.rows.iter().find(|row| row.pack_id == "3").unwrap();
    assert_eq!(orphan.reason, "orphan_hold");
    assert_eq!(orphan.written, "attempted");
    assert_eq!(plan.state, PlanState::Healthy);
    assert_eq!(plan.delivery, delivery);
    assert_eq!(plan.plan_vs_delivered.len(), 4);
    let delta: f64 = plan.plan_vs_delivered.iter().map(|row| row.delta_w).sum();
    assert_eq!(delta.abs(), 2400.0);
}

#[test]
fn s152_recovery_needs_a_measured_power_before_delivery_is_full() {
    let (plan, partial) = dropout_replay();
    assert_eq!(plan.state, PlanState::Healthy);
    assert_eq!(partial.outcome, DeliveryOutcome::Partial);
    let measured = requested_packs();
    let recovered = classify_delivery(&measured, &measured);
    assert_eq!(recovered.outcome, DeliveryOutcome::Full);
    assert_eq!(recovered.applied_w, -3200.0);
    assert_eq!(recovered.shortfall_w, 0.0);
    assert_eq!(recovered.failed_pack_ids.len(), 0);
}

#[test]
fn s78_deadline_outside_horizon_degrades_with_full_evidence() {
    let t0 = ts("2026-11-22T15:45:00Z");
    let settlements: Vec<_> = prices("2026-11-22T15:45:00Z", 20, 0.4)
        .iter()
        .map(|slot| settlement(slot.t_start, 0.4, 0.18, true))
        .collect();
    let horizon = bare_horizon(t0, settlements);
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .map(|slot| flat(slot.t_start, 6.995, 0.0, 0.0))
        .collect();
    let mut input = plan_input(horizon, timeline);
    input.obligations.push(Obligation {
        pack_id: "pack_3".to_owned(),
        required_kwh: 1.152,
        available_kwh: 0.0,
        reason: "horizon_too_short",
        deadline: "23:30 local".to_owned(),
        class: ObligationClass::Required,
        t_start: None,
        eligible: true,
    });
    assert!(input.obligations[0].eligible);
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::DegradedAccepted);
    assert_eq!(
        plan.dispatch_permission_at(plan.generated_at),
        DispatchPermission::ScopedEconomicAndSafety
    );
    let row = plan
        .unmet
        .iter()
        .find(|row| row.reason == "horizon_too_short")
        .unwrap();
    assert_eq!(row.pack_id, "pack_3");
    assert_eq!(row.deadline, "23:30 local");
    assert_eq!(row.required_kwh, 1.152);
    assert_eq!(row.available_kwh, 0.0);
    assert_eq!(row.shortfall_kwh, 1.152);
    assert_eq!(row.class, ObligationClass::Optional);
    assert_eq!(row.t_start, None);
    assert!(!row.assignment_slot_missing);
}

#[test]
fn s161_generated_timelines_conserve_energy_and_infeasible_cases_are_typed() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut seed = 0x5eed_1234u64;
    let mut accepted = 0;
    let mut refused = 0;
    for case in 0..400 {
        let timeline = generated_timeline(&horizon.slots, &mut seed);
        let mut input = plan_input(horizon.clone(), timeline);
        let valid = case < 200;
        if !valid {
            input.slots[case % 120].grid_w = 7600.0;
        }
        let plan = assemble_plan(&input).unwrap();
        if valid {
            assert_eq!(plan.state, PlanState::Healthy, "case {case}");
            assert_eq!(
                plan.dispatch_permission_at(plan.generated_at),
                DispatchPermission::EconomicAndSafety
            );
            assert!(plan.binding_bounds.is_empty(), "case {case}");
            assert!(all_slots_in_bounds(&plan, &input), "case {case}");
            accepted += 1;
        } else {
            assert_eq!(plan.state, PlanState::Infeasible, "case {case}");
            assert!(!plan.reasons.is_empty(), "case {case}");
            assert!(!plan.binding_bounds.is_empty(), "case {case}");
            assert!(
                !plan.economic_command_allowed(plan.generated_at),
                "case {case}"
            );
            refused += 1;
        }
    }
    assert_eq!(accepted, 200);
    assert_eq!(refused, 200);
}

fn requested_packs() -> Vec<PackPower> {
    ["1", "2", "3", "4"]
        .iter()
        .map(|id| PackPower {
            pack_id: (*id).to_owned(),
            w: -800.0,
        })
        .collect()
}

fn dropout_replay() -> (
    energy_decide::plan_assemble::Plan,
    energy_decide::plan_assemble::Delivery,
) {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .map(|slot| flat(slot.t_start, 6.995, 0.0, 0.0))
        .collect();
    let planned = requested_packs();
    let measured: Vec<_> = planned
        .iter()
        .filter(|pack| pack.pack_id != "3")
        .map(|pack| PackPower {
            pack_id: pack.pack_id.clone(),
            w: if pack.pack_id == "1" { -800.0 } else { 0.0 },
        })
        .collect();
    let delivery = classify_delivery(&planned, &measured);
    let mut input = plan_input(horizon, timeline);
    input.delivery = delivery.clone();
    (assemble_plan(&input).unwrap(), delivery)
}

fn next(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *seed >> 32
}

fn generated_timeline(
    times: &[energy_decide::horizon::PriceSlot],
    seed: &mut u64,
) -> Vec<energy_decide::plan_assemble::TimelineSlot> {
    let mut energy = 4.0;
    let mut timeline = Vec::with_capacity(times.len());
    for slot in times {
        let battery = (next(seed) % 81) as f64 - 40.0;
        let delta = if battery >= 0.0 {
            battery * 0.25 * 0.9 / 1000.0
        } else {
            battery * 0.25 / 0.9 / 1000.0
        };
        let end = energy + delta;
        let pv = (next(seed) % 2000) as f64;
        let load = (next(seed) % 2000) as f64;
        let mut row = flat(slot.t_start, energy, battery, load - pv + battery);
        row.e_end_kwh = end;
        row.pv_w = pv;
        row.load_w = load;
        row.pack_powers.push(PackPower {
            pack_id: "fleet".to_owned(),
            w: battery,
        });
        timeline.push(row);
        energy = end;
    }
    timeline
}

fn expected_end(slot: &energy_decide::plan_assemble::TimelineSlot) -> f64 {
    if slot.battery_w >= 0.0 {
        slot.e_start_kwh + slot.battery_w * 0.25 * 0.9 / 1000.0
    } else {
        slot.e_start_kwh + slot.battery_w * 0.25 / 0.9 / 1000.0
    }
}

fn all_slots_in_bounds(
    plan: &energy_decide::plan_assemble::Plan,
    input: &energy_decide::plan_assemble::PlanInput,
) -> bool {
    plan.timeline.iter().all(|slot| {
        (input.bounds.floor_kwh..=input.bounds.ceiling_kwh).contains(&slot.e_end_kwh)
            && slot.grid_w <= input.bounds.import_ceiling_w
            && slot.grid_w >= -input.bounds.export_ceiling_w
            && (slot.e_end_kwh - expected_end(slot)).abs() <= 0.001
    })
}
