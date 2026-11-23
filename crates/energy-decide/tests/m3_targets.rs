//! M1C regression pins: absent pack delivery and full target-field validation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::plan_assemble::{
    AssembleRefusal, Obligation, ObligationClass, PlanState, TimelineSlot, assemble_plan,
};
use support::{assembled, flat, plan_input};

fn flat_timeline(slots: &[energy_decide::horizon::PriceSlot]) -> Vec<TimelineSlot> {
    slots
        .iter()
        .map(|slot| flat(slot.t_start, 6.995, 0.0, 0.0))
        .collect()
}

#[test]
fn m1c_absent_pack_delivers_zero_not_unknown() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline = flat_timeline(&horizon.slots);
    let mut input = plan_input(horizon, timeline);
    input.obligations.push(Obligation {
        pack_id: "never_delivered".to_owned(),
        required_kwh: 1.0,
        available_kwh: 1.0,
        reason: "clamp",
        deadline: "07:00 local".to_owned(),
        class: ObligationClass::Required,
        t_start: None,
        eligible: true,
    });
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Infeasible);
    assert_ne!(plan.state, PlanState::Healthy);
    let unmet = plan
        .unmet
        .iter()
        .find(|row| row.pack_id == "never_delivered")
        .unwrap();
    assert_eq!(unmet.available_kwh, 0.0);
    assert_eq!(unmet.shortfall_kwh, 1.0);
    assert_eq!(unmet.reason, "clamp");
    assert!(
        plan.binding_bounds
            .iter()
            .any(|row| row.reason == "obligation_delivery")
    );
}

#[test]
fn soft_guideline_above_ceiling_is_clamped_not_refused() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline: Vec<_> = horizon
        .slots
        .iter()
        .map(|slot| flat(slot.t_start, 0.95, 0.0, 0.0))
        .collect();
    let mut input = plan_input(horizon, timeline);
    input.bounds.floor_kwh = 0.5;
    input.bounds.ceiling_kwh = 1.0;
    input.targets.soft_guideline_kwh = 1.5;
    input.targets.hard_floor_kwh = 0.5;
    input.targets.ceiling_kwh = 1.0;
    let plan = assemble_plan(&input).unwrap();
    assert_eq!(plan.state, PlanState::Healthy);
    assert_eq!(plan.effective_target_kwh, 1.0);
    assert_eq!(plan.raw_guideline_kwh, 1.5);
}

#[test]
fn m1c_every_target_field_rejects_non_finite_values() {
    let horizon = assembled("2026-11-22T15:47:00Z", 120);
    let timeline = flat_timeline(&horizon.slots);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in [
            "soft_guideline_kwh",
            "hard_floor_kwh",
            "ceiling_kwh",
            "learned_capacity_kwh",
            "label_capacity_kwh",
        ] {
            let mut input = plan_input(horizon.clone(), timeline.clone());
            match field {
                "soft_guideline_kwh" => input.targets.soft_guideline_kwh = bad,
                "hard_floor_kwh" => input.targets.hard_floor_kwh = bad,
                "ceiling_kwh" => input.targets.ceiling_kwh = bad,
                "learned_capacity_kwh" => input.targets.learned_capacity_kwh = bad,
                "label_capacity_kwh" => input.targets.label_capacity_kwh = bad,
                _ => unreachable!("field list"),
            }
            assert!(
                matches!(
                    assemble_plan(&input),
                    Err(AssembleRefusal::InvalidTargets { .. })
                ),
                "{field} accepted {bad}"
            );
        }
        let mut input = plan_input(horizon.clone(), timeline.clone());
        input.targets.terminal_target_pct = Some(bad);
        assert!(
            matches!(
                assemble_plan(&input),
                Err(AssembleRefusal::TerminalTargetPct { .. })
            ),
            "terminal_target_pct accepted {bad}"
        );
    }
    let mut input = plan_input(horizon, timeline);
    input.targets.hard_floor_kwh = 9.0;
    input.targets.ceiling_kwh = 8.0;
    assert!(matches!(
        assemble_plan(&input),
        Err(AssembleRefusal::InvalidTargets { .. })
    ));
}
