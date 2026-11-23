//! M3 scenarios S22 and S78-S80: maintenance assignment and acceptance lifecycle.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::plan_assemble::{
    Acceptance, DispatchPermission, Obligation, ObligationClass, PlanState, assemble_plan,
};
use support::{assembled, bare_horizon, flat, input, plan_input, prices, settlement, ts};

#[test]
fn s22_assignment_slot_missing_is_unassigned_not_silently_continued() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let assignment = ts("2026-11-23T02:00:00Z");
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .filter(|slot| slot.t_start != assignment)
        .map(|slot| flat(slot.t_start, 6.995, 0.0, 0.0))
        .collect();
    let mut plan_input = plan_input(horizon, timeline);
    plan_input.obligations.push(Obligation {
        pack_id: "pack_3".to_owned(),
        required_kwh: 1.152,
        available_kwh: 0.0,
        reason: "horizon_too_short",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Optional,
        t_start: Some(assignment),
        eligible: false,
    });
    let plan = assemble_plan(&plan_input).unwrap();
    let row = plan
        .unmet
        .iter()
        .find(|row| row.reason == "assignment_slot_missing")
        .unwrap();
    assert_eq!(row.pack_id, "pack_3");
    assert_eq!(row.t_start, Some(assignment));
    assert_eq!(row.deadline, "07:00 local");
    let named = plan
        .unmet
        .iter()
        .find(|row| row.reason == "horizon_too_short")
        .unwrap();
    assert_eq!(named.pack_id, "pack_3");
    assert_eq!(named.class, ObligationClass::Optional);
    assert_eq!(named.deadline, "07:00 local");
}

#[test]
fn s78_deadline_outside_horizon_degrades_with_full_evidence() {
    // A 20-slot horizon ends at 22:45 local, before the 23:30 local deadline.
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
    let mut plan_input = plan_input(horizon, timeline);
    plan_input.obligations.push(Obligation {
        pack_id: "pack_3".to_owned(),
        required_kwh: 1.152,
        available_kwh: 0.0,
        reason: "horizon_too_short",
        deadline: "23:30 local".to_owned(),
        class: ObligationClass::Required,
        t_start: None,
        eligible: true,
    });
    let plan = assemble_plan(&plan_input).unwrap();
    assert_eq!(plan.state, PlanState::DegradedAccepted);
    assert_eq!(
        plan.dispatch_permission_at(plan.generated_at),
        DispatchPermission::ScopedEconomicAndSafety
    );
    let row = plan.unmet.first().unwrap();
    assert_eq!(row.reason, "horizon_too_short");
    assert_eq!(row.pack_id, "pack_3");
    assert_eq!(row.deadline, "23:30 local");
    assert_eq!(row.class, ObligationClass::Optional);
    assert_eq!(row.required_kwh, 1.152);
    assert_eq!(row.available_kwh, 0.0);
    assert_eq!(row.shortfall_kwh, 1.152);
}

#[test]
fn s79_maintenance_below_floor_makes_the_assembled_plan_infeasible() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let broken_energy = 1.178625 - 1.0;
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .map(|slot| flat(slot.t_start, broken_energy, 0.0, 0.0))
        .collect();
    let mut plan_input = plan_input(horizon, timeline);
    plan_input.obligations.push(Obligation {
        pack_id: "pack_3".to_owned(),
        required_kwh: 1.152,
        available_kwh: 1.152,
        reason: "maintenance",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: None,
        eligible: true,
    });
    let plan = assemble_plan(&plan_input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert_eq!(
        plan.dispatch_permission_at(plan.generated_at),
        DispatchPermission::SafetyOnly
    );
    assert!(
        plan.binding_bounds
            .iter()
            .any(|row| row.reason == "soc_floor")
    );
    assert!(plan.reasons.contains(&"maintenance_broke_floor"));
    assert!(
        plan.unmet
            .iter()
            .any(|row| row.reason == "maintenance_broke_floor")
    );
}

#[test]
fn s80_acceptance_expires_and_the_plan_does_not_continue_degraded() {
    let request = input(
        "2026-11-22T15:47:00Z",
        prices("2026-11-22T15:45:00Z", 108, 0.4),
    );
    let attempt = energy_decide::horizon::assemble_horizon(&request);
    let horizon = attempt.horizon().unwrap().clone();
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .map(|slot| flat(slot.t_start, 2.0, 0.0, 0.0))
        .collect();
    let generated = horizon.t0 + 60;
    let mut plan_input = plan_input(horizon, timeline);
    plan_input.generated_at = generated;
    plan_input.acceptance = Some(Acceptance {
        scope: "tail_24h_only".to_owned(),
        accepted_by: "operator".to_owned(),
        expires_at: generated + 900,
    });
    let plan = assemble_plan(&plan_input).unwrap();
    assert_eq!(plan.state, PlanState::DegradedAccepted);
    assert_eq!(
        plan.acceptance
            .as_ref()
            .map_or(0, |a| a.expires_at - generated),
        900
    );
    assert_eq!(plan.state_at(generated + 899), PlanState::DegradedAccepted);
    assert_eq!(plan.state_at(generated + 900), PlanState::Infeasible);
    assert_eq!(
        plan.dispatch_permission_at(generated + 900),
        DispatchPermission::SafetyOnly
    );
    assert!(!plan.economic_command_allowed(generated + 900));
}
