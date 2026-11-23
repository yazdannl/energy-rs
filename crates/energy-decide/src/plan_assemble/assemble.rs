//! Assemble the final plan. Bounds are checked before anything else; refusals stay typed.

use crate::horizon::HorizonOutcome;
use crate::plan_assemble::{
    AssembleRefusal, ObligationClass, Plan, PlanInput, PlanState, PlanVsDelivered, Unmet,
    summarize, validate_timeline,
};

/// Validate and assemble a plan; no timeline is reused when a bound conflict is refused.
pub fn assemble_plan(input: &PlanInput) -> Result<Plan, AssembleRefusal> {
    input.bounds.validate()?;
    input.targets.validate_finite()?;
    input.targets.validate_pct()?;
    let validation = validate_timeline(input);
    let mut reasons = validation.reasons;
    let mut unmet = validation.unmet;
    let mut hard = validation.hard_violation;
    let mut degraded = validation.degraded;
    let mut scope = "optional_obligation";
    if let Some(refusal) = input.refusal.as_ref() {
        hard = true;
        reasons.push(refusal.reason_slug());
    }
    if input.horizon.outcome == HorizonOutcome::DegradedAccepted || input.horizon.coverage.is_tail()
    {
        degraded = true;
        scope = "tail_24h_only";
        reasons.push("insufficient_price_coverage");
        unmet.push(coverage_unmet(input));
    }
    let state = if hard {
        PlanState::Infeasible
    } else if degraded {
        PlanState::DegradedAccepted
    } else {
        PlanState::Healthy
    };
    let summary = summarize(
        &input.slots,
        &input.horizon.net_metering,
        input.targets.learned_capacity_kwh,
        input.bounds.slot_seconds,
    );
    let mut plan = Plan {
        generated_at: input.generated_at,
        horizon_start: input.horizon.t0,
        horizon_end: input.horizon.coverage_end(),
        requested_slots: input.horizon.requested_slots,
        actual_slots: input.horizon.actual_slots(),
        state,
        reasons,
        binding_bounds: validation.binding_bounds,
        unmet,
        plan_vs_delivered: delivery_rows(input),
        grid_source_coverage: input.horizon.grid_source_coverage.clone(),
        timeline: input.slots.clone(),
        summary,
        acceptance: input.acceptance.clone(),
        delivery: input.delivery.clone(),
        effective_target_kwh: input.targets.effective_kwh(),
        raw_guideline_kwh: input.targets.soft_guideline_kwh,
        terminal_shortfall_kwh: validation.terminal_shortfall_kwh,
        degraded,
        degraded_scope: scope,
    };
    let settled = plan.state_at(input.generated_at);
    plan.state = settled;
    Ok(plan)
}

fn coverage_unmet(input: &PlanInput) -> Unmet {
    let missing = input.horizon.coverage.missing_slots;
    Unmet {
        obligation: "price_coverage".to_owned(),
        pack_id: "fleet".to_owned(),
        required_kwh: input.horizon.coverage.requested_slots as f64,
        available_kwh: input.horizon.coverage.actual_slots as f64,
        shortfall_kwh: missing as f64,
        reason: "insufficient_price_coverage",
        deadline: "horizon_end".to_owned(),
        class: ObligationClass::Optional,
        t_start: input.horizon.coverage.first_missing_t_start,
        assignment_slot_missing: false,
    }
}

fn delivery_rows(input: &PlanInput) -> Vec<PlanVsDelivered> {
    let slot = input
        .slots
        .first()
        .map_or(input.horizon.t0, |row| row.t_start);
    input
        .delivery
        .rows
        .iter()
        .map(|row| PlanVsDelivered {
            slot,
            planned_w: row.planned_w,
            delivered_w: row.applied_w,
            delta_w: row.planned_w - row.applied_w,
        })
        .collect()
}
