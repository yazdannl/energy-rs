//! M3 scenarios S19-S21, S75-S77 and S128-S129: states, bounds, targets, shortfalls.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::plan_assemble::{
    AssembleRefusal, DispatchPermission, Obligation, ObligationClass, PackPower, PlanState,
    TimelineSlot, assemble_plan,
};
use support::{assembled, bare_horizon, flat, input, plan_input, prices, settlement, ts};

#[test]
fn s19_summary_counts_pv_charge_and_nets_the_equal_price_wash() {
    let t0 = ts("2026-11-22T15:45:00Z");
    let horizon = bare_horizon(
        t0,
        vec![
            settlement(t0, 0.40, 0.0, true),
            settlement(t0 + 900, 0.40, 0.40, true),
            settlement(t0 + 1800, 0.40, 0.40, true),
        ],
    );
    let mut pv_slot = flat(t0, 3.0, 300.0, 0.0);
    pv_slot.pv_w = 500.0;
    pv_slot.load_w = 200.0;
    pv_slot.e_end_kwh = 3.0675;
    pv_slot.pack_powers.push(PackPower {
        pack_id: "fleet".to_owned(),
        w: 300.0,
    });
    let mut import_slot = flat(t0 + 900, 3.0675, 0.0, 4000.0);
    import_slot.load_w = 4000.0;
    import_slot.e_end_kwh = 3.0675;
    let mut export_slot = flat(t0 + 1800, 3.0675, 0.0, -4000.0);
    export_slot.pv_w = 4000.0;
    export_slot.e_end_kwh = 3.0675;
    let plan = assemble_plan(&plan_input(
        horizon,
        vec![pv_slot, import_slot, export_slot],
    ))
    .unwrap();
    assert_eq!(plan.summary.pv_charge_kwh, 0.075);
    assert_eq!(plan.summary.grid_charge_kwh, 0.0);
    assert_eq!(plan.summary.saved_eur, 0.0);
    assert_eq!(plan.summary.cost_eur, 0.0);
}

#[test]
fn s20_required_shortfall_from_clamp_is_infeasible_but_timeline_is_kept() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.0);
    apply_packs(
        &mut timeline,
        &[
            (0, "pack_a", 800.0),
            (1, "pack_a", 800.0),
            (2, "pack_a", 800.0),
            (3, "pack_a", 800.0),
            (4, "pack_a", 800.0),
        ],
    );
    let mut input = plan_input(horizon, timeline);
    input.targets.soft_guideline_kwh = 0.15 * 7.68;
    input.obligations.push(Obligation {
        pack_id: "pack_a".to_owned(),
        required_kwh: 1.15,
        available_kwh: 0.75,
        reason: "clamp",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: Some(ts("2026-11-23T02:00:00Z")),
        eligible: true,
    });
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert_eq!(plan.timeline.len(), 120);
    let unmet = plan
        .unmet
        .iter()
        .find(|row| row.pack_id == "pack_a")
        .unwrap();
    assert_eq!(unmet.shortfall_kwh, 0.4);
    assert_eq!(unmet.reason, "clamp");
    assert!(plan.binding_bounds.iter().any(|row| row.reason == "clamp"));
}

#[test]
fn s21_invalid_target_percentages_are_refused_below_floor_is_clamped() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline = flat_timeline(&horizon_slots(120), 2.0);
    for pct in [150.0, -1.0] {
        let mut input = plan_input(horizon.clone(), timeline.clone());
        input.targets.terminal_target_pct = Some(pct);
        assert_eq!(
            assemble_plan(&input),
            Err(AssembleRefusal::TerminalTargetPct { pct })
        );
    }
    let mut input = plan_input(horizon, timeline);
    input.targets.terminal_target_pct = Some(5.0);
    input.targets.soft_guideline_kwh = 0.05 * 7.8575;
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Healthy);
    assert_eq!(plan.effective_target_kwh, 1.178625);
}

#[test]
fn s75_floor_above_ceiling_is_refused_before_solving() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline = flat_timeline(&horizon_slots(120), 2.0);
    let mut input = plan_input(horizon, timeline);
    // A poisoned timeline must not hide the pre-solve bounds refusal.
    input.slots[0].e_start_kwh = f64::NAN;
    input.bounds.floor_kwh = 2.5;
    input.bounds.ceiling_kwh = 2.0;
    assert_eq!(
        assemble_plan(&input),
        Err(AssembleRefusal::InvalidBounds {
            floor_kwh: 2.5,
            ceiling_kwh: 2.0,
        })
    );
}

#[test]
fn s76_learned_target_makes_final_energy_healthy() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let plan = assemble_plan(&plan_input(
        horizon,
        flat_timeline(&horizon_slots(120), 2.0),
    ))
    .unwrap();
    assert_eq!(plan.state, PlanState::Healthy);
    assert_eq!(plan.effective_target_kwh, 1.178625);
    assert_eq!(plan.raw_guideline_kwh, 0.78575);
    assert_eq!(plan.terminal_shortfall_kwh, 0.0);
    assert!(!plan.reasons.contains(&"terminal_target_unreachable"));
}

#[test]
fn s77_pv_headroom_clip_is_a_shortfall_and_an_infeasible_plan() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.5);
    apply_packs(
        &mut timeline,
        &[
            (0, "pack_a", 800.0),
            (1, "pack_a", 800.0),
            (2, "pack_a", 800.0),
            (3, "pack_a", 800.0),
        ],
    );
    timeline[0].pv_headroom_kwh = 0.6;
    let mut input = plan_input(horizon, timeline);
    input.obligations.push(Obligation {
        pack_id: "pack_a".to_owned(),
        required_kwh: 1.0,
        available_kwh: 1.0,
        reason: "grid_charge",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: Some(ts("2026-11-22T15:45:00Z")),
        eligible: true,
    });
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    let unmet = plan.unmet.first().unwrap();
    assert_eq!(unmet.required_kwh, 1.0);
    assert_eq!(unmet.available_kwh, 0.6);
    assert_eq!(unmet.shortfall_kwh, 0.4);
    assert!(
        plan.binding_bounds
            .iter()
            .any(|row| row.reason == "pv_headroom")
    );
    assert_eq!(
        plan.dispatch_permission_at(plan.generated_at),
        DispatchPermission::SafetyOnly
    );
}

#[test]
fn s128_terminal_validation_uses_actual_slot_index() {
    let mut request = input(
        "2026-11-22T15:47:00Z",
        prices("2026-11-22T15:45:00Z", 96, 0.4),
    );
    request.requested_slots = 96;
    let attempt = energy_decide::horizon::assemble_horizon(&request);
    let horizon = attempt.horizon().unwrap().clone();
    let mut timeline = flat_timeline(&horizon_slots(96), 2.0);
    let last = timeline.last_mut().unwrap();
    last.battery_w = -3060.0;
    last.grid_w = -3060.0;
    last.e_end_kwh = 1.150;
    last.pack_powers.push(PackPower {
        pack_id: "fleet".to_owned(),
        w: -3060.0,
    });
    let plan = assemble_plan(&plan_input(horizon, timeline)).unwrap();
    assert_eq!(plan.actual_slots, 96);
    assert_eq!(plan.timeline.len(), 96);
    assert_eq!(plan.state, PlanState::Healthy);
    assert_eq!(plan.raw_guideline_kwh, 0.78575);
    assert!(plan.terminal_shortfall_kwh <= 0.05);
    assert!(!plan.reasons.contains(&"terminal_target_unreachable"));
}

#[test]
fn s129_required_and_optional_unmet_rows_keep_reason_deadline_and_class() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.0);
    apply_packs(
        &mut timeline,
        &[
            (0, "A", 800.0),
            (1, "A", 800.0),
            (2, "A", 800.0),
            (3, "A", 800.0),
            (4, "B", 800.0),
            (5, "B", 800.0),
        ],
    );
    let mut input = plan_input(horizon, timeline);
    input.obligations.push(Obligation {
        pack_id: "A".to_owned(),
        required_kwh: 1.0,
        available_kwh: 0.6,
        reason: "insufficient_capacity",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: None,
        eligible: true,
    });
    input.obligations.push(Obligation {
        pack_id: "B".to_owned(),
        required_kwh: 0.4,
        available_kwh: 0.2,
        reason: "no_cheap_slots",
        deadline: "06:00 local".to_owned(),
        class: ObligationClass::Optional,
        t_start: None,
        eligible: false,
    });
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert_eq!(plan.unmet.len(), 2);
    let a = plan.unmet.iter().find(|row| row.pack_id == "A").unwrap();
    assert_eq!(a.shortfall_kwh, 0.4);
    assert_eq!(a.reason, "insufficient_capacity");
    assert_eq!(a.deadline, "07:00 local");
    assert_eq!(a.class, ObligationClass::Required);
    let b = plan.unmet.iter().find(|row| row.pack_id == "B").unwrap();
    assert_eq!(b.shortfall_kwh, 0.2);
    assert_eq!(b.reason, "no_cheap_slots");
    assert_eq!(b.deadline, "06:00 local");
    assert_eq!(b.class, ObligationClass::Optional);
}

#[test]
fn m1c_timeline_order_and_continuity_are_validated() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.995);
    timeline[5].e_start_kwh = 9.0;
    timeline[10].t_start += 1;
    let plan = assemble_plan(&plan_input(horizon, timeline)).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert!(plan.reasons.contains(&"timeline_discontinuity"));
    assert!(plan.reasons.contains(&"timeline_order"));
    assert!(
        plan.binding_bounds
            .iter()
            .any(|row| row.reason == "timeline_discontinuity")
    );
}

#[test]
fn m1c_aggregate_and_grid_balances_are_enforced() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.995);
    timeline[0].battery_w = 4000.0;
    timeline[0].grid_w = 4000.0;
    timeline[1].grid_w = 5000.0;
    let plan = assemble_plan(&plan_input(horizon, timeline)).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    for reason in ["power_clamp", "pack_battery_balance", "grid_balance"] {
        assert!(plan.reasons.contains(&reason), "missing {reason}");
        assert!(
            plan.binding_bounds.iter().any(|row| row.reason == reason),
            "missing binding {reason}"
        );
    }
}

#[test]
fn m1c_non_finite_inputs_are_refused_not_ignored() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline = flat_timeline(&horizon_slots(120), 6.995);
    let mut input = plan_input(horizon.clone(), timeline.clone());
    input.bounds.import_ceiling_w = f64::NAN;
    assert_eq!(
        assemble_plan(&input),
        Err(AssembleRefusal::InvalidBounds {
            floor_kwh: input.bounds.floor_kwh,
            ceiling_kwh: input.bounds.ceiling_kwh,
        })
    );
    let mut input = plan_input(horizon.clone(), timeline.clone());
    input.targets.soft_guideline_kwh = f64::NAN;
    assert!(matches!(
        assemble_plan(&input),
        Err(AssembleRefusal::InvalidTargets { .. })
    ));
    let mut input = plan_input(horizon, timeline);
    input.bounds.eta_charge = 0.0;
    assert!(matches!(
        assemble_plan(&input),
        Err(AssembleRefusal::InvalidBounds { .. })
    ));
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.995);
    timeline[3].grid_w = f64::NAN;
    let plan = assemble_plan(&plan_input(horizon, timeline)).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert!(plan.reasons.contains(&"non_finite_timeline"));
}

#[test]
fn m1c_terminal_deadband_is_deducted_from_the_reported_shortfall() {
    for (end, expected) in [(1.1, 0.028625), (1.128625, 0.0)] {
        let horizon = assembled("2026-11-22T15:47:00Z", 120);
        let mut timeline = flat_timeline(&horizon_slots(120), end + 0.85);
        let last = timeline.last_mut().unwrap();
        last.e_start_kwh = end + 0.85;
        last.e_end_kwh = end;
        last.battery_w = -3060.0;
        last.grid_w = -3060.0;
        last.pack_powers.push(PackPower {
            pack_id: "fleet".to_owned(),
            w: -3060.0,
        });
        let mut input = plan_input(horizon, timeline);
        input.targets.terminal_target_pct = Some(5.0);
        let plan = assemble_plan(&input).unwrap();
        assert_eq!(plan.effective_target_kwh, 1.178625);
        assert_eq!(plan.terminal_shortfall_kwh, expected);
    }
}

#[test]
fn m1c_stale_availability_is_reconciled_with_delivered_energy() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let mut timeline = flat_timeline(&horizon_slots(120), 6.995);
    timeline[0].battery_w = 800.0;
    timeline[0].grid_w = 800.0;
    timeline[0].e_end_kwh = 7.175;
    timeline[0].pack_powers.push(PackPower {
        pack_id: "pack_a".to_owned(),
        w: 800.0,
    });
    for slot in timeline.iter_mut().skip(1) {
        slot.e_start_kwh = 7.175;
        slot.e_end_kwh = 7.175;
    }
    let mut input = plan_input(horizon, timeline);
    input.obligations.push(Obligation {
        pack_id: "pack_a".to_owned(),
        required_kwh: 1.0,
        available_kwh: 1.0,
        reason: "clamp",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: Some(ts("2026-11-22T15:45:00Z")),
        eligible: true,
    });
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    let unmet = plan
        .unmet
        .iter()
        .find(|row| row.pack_id == "pack_a")
        .unwrap();
    assert_eq!(unmet.available_kwh, 0.18);
    assert_eq!(unmet.shortfall_kwh, 0.82);
    assert_eq!(unmet.reason, "clamp");
    assert!(
        plan.binding_bounds
            .iter()
            .any(|row| row.reason == "obligation_delivery")
    );
}

#[test]
fn m1c_missing_required_assignment_is_infeasible_and_keeps_named_reason() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let assignment = ts("2026-11-23T02:00:00Z");
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .filter(|slot| slot.t_start != assignment)
        .map(|slot| flat(slot.t_start, 6.995, 0.0, 0.0))
        .collect();
    let mut input = plan_input(horizon, timeline);
    input.obligations.push(Obligation {
        pack_id: "pack_3".to_owned(),
        required_kwh: 1.152,
        available_kwh: 0.0,
        reason: "horizon_too_short",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: Some(assignment),
        eligible: true,
    });
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert!(plan.reasons.contains(&"required_assignment_missing"));
    let named = plan
        .unmet
        .iter()
        .find(|row| row.reason == "horizon_too_short")
        .unwrap();
    assert_eq!(named.class, ObligationClass::Required);
    assert_eq!(named.deadline, "07:00 local");
    assert_eq!(named.shortfall_kwh, 1.152);
    let missing = plan
        .unmet
        .iter()
        .find(|row| row.reason == "assignment_slot_missing")
        .unwrap();
    assert!(missing.assignment_slot_missing);
    assert_eq!(missing.t_start, Some(assignment));
}

fn apply_packs(timeline: &mut [TimelineSlot], rows: &[(usize, &str, f64)]) {
    let mut packs: Vec<Vec<PackPower>> = vec![Vec::new(); timeline.len()];
    for &(index, pack_id, watts) in rows {
        if let Some(slot_packs) = packs.get_mut(index) {
            slot_packs.push(PackPower {
                pack_id: pack_id.to_owned(),
                w: watts,
            });
        }
    }
    let mut energy = timeline.first().map_or(0.0, |slot| slot.e_start_kwh);
    for (index, slot) in timeline.iter_mut().enumerate() {
        slot.pack_powers.clone_from(&packs[index]);
        let battery: f64 = slot.pack_powers.iter().map(|pack| pack.w).sum();
        slot.battery_w = battery;
        slot.grid_w = battery;
        slot.e_start_kwh = energy;
        let delta = if battery >= 0.0 {
            battery * 0.25 * 0.9 / 1000.0
        } else {
            battery * 0.25 / 1000.0 / 0.9
        };
        slot.e_end_kwh = energy + delta;
        energy = slot.e_end_kwh;
    }
}

fn horizon_slots(count: usize) -> Vec<i64> {
    let start = ts("2026-11-22T15:45:00Z");
    (0..count)
        .map(|index| start + i64::try_from(index).unwrap_or(i64::MAX) * 900)
        .collect()
}

fn flat_timeline(times: &[i64], energy: f64) -> Vec<energy_decide::plan_assemble::TimelineSlot> {
    times
        .iter()
        .map(|time| flat(*time, energy, 0.0, 0.0))
        .collect()
}
