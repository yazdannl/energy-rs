//! Final timeline validation after solver, clamp, maintenance and SoC recomputation.

use crate::plan_assemble::delivery_check::delivered_until;
use crate::plan_assemble::validation_checks::{check_shape, check_slot, slot_is_finite};
use crate::plan_assemble::{
    BindingBound, Obligation, ObligationClass, PlanInput, Unmet, obligation_class,
};

/// Every binding bound, shortfall and reason found on the final timeline.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Validation {
    /// Bounds that bound or clipped the plan.
    pub binding_bounds: Vec<BindingBound>,
    /// Required and optional shortfalls.
    pub unmet: Vec<Unmet>,
    /// Ordered reason slugs.
    pub reasons: Vec<&'static str>,
    /// Whether a hard constraint was violated.
    pub hard_violation: bool,
    /// Whether an optional obligation or degraded horizon is unmet.
    pub degraded: bool,
    /// One-sided terminal shortfall after the 0.05 kWh deadband deduction.
    pub terminal_shortfall_kwh: f64,
}

/// Validate every slot, the terminal state and all obligations.
pub fn validate_timeline(input: &PlanInput) -> Validation {
    let mut result = Validation::default();
    check_shape(input, &mut result);
    let target = input.targets.effective_kwh();
    let last_index = input.slots.len().saturating_sub(1);
    for (index, slot) in input.slots.iter().enumerate() {
        if !slot_is_finite(slot) {
            result.binding(binding("non_finite_timeline", Some(slot.t_start), f64::NAN));
            result.hard("non_finite_timeline");
            continue;
        }
        let tolerated =
            index == last_index && slot.e_end_kwh >= target - crate::TERMINAL_TOLERANCE_KWH;
        check_slot(slot, &input.bounds, tolerated, &mut result);
    }
    check_terminal(input, &mut result);
    for obligation in &input.obligations {
        check_obligation(obligation, input, &mut result);
    }
    result
}

fn check_terminal(input: &PlanInput, result: &mut Validation) {
    let Some(last) = input.slots.last() else {
        result.hard("missing_timeline");
        return;
    };
    let target = input.targets.effective_kwh();
    let gap = target - last.e_end_kwh - crate::TERMINAL_TOLERANCE_KWH;
    result.terminal_shortfall_kwh = round_kwh(gap.max(0.0));
    if result.terminal_shortfall_kwh > 0.0 {
        result.binding(binding("terminal_target", Some(last.t_start), target));
        result.unmet.push(Unmet {
            obligation: "terminal_target".to_owned(),
            pack_id: "fleet".to_owned(),
            required_kwh: target,
            available_kwh: last.e_end_kwh,
            shortfall_kwh: result.terminal_shortfall_kwh,
            reason: "terminal_target_unreachable",
            deadline: "horizon_end".to_owned(),
            class: ObligationClass::Required,
            t_start: Some(last.t_start),
            assignment_slot_missing: false,
        });
        result.hard("terminal_target_unreachable");
    }
}

fn check_obligation(obligation: &Obligation, input: &PlanInput, result: &mut Validation) {
    if !obligation.required_kwh.is_finite() || !obligation.available_kwh.is_finite() {
        result.hard("non_finite_obligation");
        return;
    }
    let class = obligation_class(obligation, &input.horizon);
    let slot = obligation
        .t_start
        .and_then(|time| input.slots.iter().find(|row| row.t_start == time));
    if obligation.t_start.is_some() && slot.is_none() {
        result.unmet.push(unmet_row(
            obligation,
            class,
            obligation.reason,
            obligation.required_kwh,
            obligation.available_kwh,
        ));
        let mut row = unmet_row(
            obligation,
            class,
            "assignment_slot_missing",
            obligation.required_kwh,
            0.0,
        );
        row.assignment_slot_missing = true;
        result.unmet.push(row);
        if class == ObligationClass::Required {
            result.hard("required_assignment_missing");
        } else {
            result.degraded = true;
        }
        return;
    }
    reconcile_delivery(obligation, input, class, result);
}

fn reconcile_delivery(
    obligation: &Obligation,
    input: &PlanInput,
    class: ObligationClass,
    result: &mut Validation,
) {
    let mut available = obligation.available_kwh;
    let delivered = delivered_until(obligation, input);
    if delivered < available {
        result.binding(binding(
            "obligation_delivery",
            obligation.t_start,
            delivered,
        ));
        available = delivered;
    }
    let slot = obligation
        .t_start
        .and_then(|time| input.slots.iter().find(|row| row.t_start == time));
    if obligation.reason == "grid_charge"
        && let Some(slot) = slot
        && slot.pv_headroom_kwh < available
    {
        result.binding(binding(
            "pv_headroom",
            Some(slot.t_start),
            slot.pv_headroom_kwh,
        ));
        available = slot.pv_headroom_kwh;
    }
    let shortfall = (obligation.required_kwh - available).max(0.0);
    if shortfall > crate::TERMINAL_TOLERANCE_KWH {
        result.unmet.push(unmet_row(
            obligation,
            class,
            obligation.reason,
            obligation.required_kwh,
            available,
        ));
        result.binding(binding(obligation.reason, obligation.t_start, available));
        if class == ObligationClass::Required {
            result.hard("required_outcome_unmet");
        } else {
            result.degraded = true;
        }
    }
    check_maintenance_floor(obligation, input, class, result);
}

fn check_maintenance_floor(
    obligation: &Obligation,
    input: &PlanInput,
    class: ObligationClass,
    result: &mut Validation,
) {
    if obligation.reason != "maintenance" {
        return;
    }
    let lowest = input
        .slots
        .iter()
        .map(|row| row.e_end_kwh)
        .fold(f64::INFINITY, f64::min);
    if lowest >= input.bounds.floor_kwh {
        return;
    }
    let gap = (input.bounds.floor_kwh - lowest).max(0.0);
    result.unmet.push(unmet_row(
        obligation,
        class,
        "maintenance_broke_floor",
        input.bounds.floor_kwh,
        lowest,
    ));
    result.binding(binding("soc_floor", obligation.t_start, gap));
    result.hard("maintenance_broke_floor");
}

fn binding(name: &'static str, t_start: Option<i64>, value: f64) -> BindingBound {
    BindingBound {
        reason: name,
        slot: t_start,
        kw_or_kwh: value,
    }
}

fn round_kwh(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}

fn unmet_row(
    obligation: &Obligation,
    class: ObligationClass,
    reason: &'static str,
    required: f64,
    available: f64,
) -> Unmet {
    Unmet {
        obligation: obligation.pack_id.clone(),
        pack_id: obligation.pack_id.clone(),
        required_kwh: required,
        available_kwh: available,
        shortfall_kwh: round_kwh((required - available).max(0.0)),
        reason,
        deadline: obligation.deadline.clone(),
        class,
        t_start: obligation.t_start,
        assignment_slot_missing: false,
    }
}

impl Validation {
    pub(crate) fn binding(&mut self, bound: BindingBound) {
        self.binding_bounds.push(bound);
    }

    pub(crate) fn hard(&mut self, reason: &'static str) {
        self.reasons.push(reason);
        self.hard_violation = true;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn charge_and_discharge_use_their_own_efficiency_boundary() {
        let bounds = crate::plan_assemble::bounds::fleet_bounds();
        assert!(
            (crate::plan_assemble::validation_checks::expected_energy(3.0, 800.0, &bounds, 0.25)
                - 3.18)
                .abs()
                < 1e-12
        );
        let discharged =
            crate::plan_assemble::validation_checks::expected_energy(3.0, -800.0, &bounds, 0.25);
        assert!((discharged - (3.0 - 800.0 * 0.25 / 1000.0 / 0.9)).abs() < 1e-12);
    }
}
