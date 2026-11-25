//! Per-rule hazard predicates. The ordered evaluation lives in `ladder.rs`.

use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::dispatch::{SafetyAction, SlotRequest};
use crate::safety::rules_limits::physical_limits;
use crate::safety::state::LadderState;
use crate::safety::{
    AGGREGATE_EMERGENCY_W, AUTHORIZATION_MIN_MS, CELL_NOMINAL_V, CELL_RESUME_V, CELL_TRIGGER_V,
    COMMAND_SAMPLE_MAX_MS, EMERGENCY_DURATION_MS, HoldProof, MAX_PLAUSIBLE_POWER_W,
    RESTART_GRACE_MS, Rule, SafetyInput, fresh_enough,
};

/// One fired rule, its reason and the command or deferral it selects.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub rule: Rule,
    pub reason: &'static str,
    pub action: Option<SafetyAction>,
    pub binding_limits: Vec<&'static str>,
    pub extra_reasons: Vec<&'static str>,
    pub blocked: bool,
    pub defer_until_ms: Option<i64>,
}

pub(crate) fn candidate(rule: Rule, action: Option<SafetyAction>) -> Candidate {
    Candidate {
        rule,
        reason: rule.reason(),
        action,
        binding_limits: Vec::new(),
        extra_reasons: Vec::new(),
        blocked: false,
        defer_until_ms: None,
    }
}

pub(crate) fn blocked(rule: Rule, reason: &'static str, limits: Vec<&'static str>) -> Candidate {
    Candidate {
        rule,
        reason,
        action: Some(SafetyAction::NeutralHold),
        binding_limits: limits,
        extra_reasons: Vec::new(),
        blocked: true,
        defer_until_ms: None,
    }
}

pub(crate) fn hold(rule: Rule) -> Candidate {
    candidate(rule, Some(SafetyAction::NeutralHold))
}

/// Load minus PV after removing the whole measured fleet battery contribution.
/// `battery_w` is fleet-total; adding the complete selected targets then includes each command once.
pub(crate) fn fleet_baseline_w(input: &SafetyInput) -> Option<f64> {
    input
        .grid_w
        .zip(input.battery_w)
        .map(|(grid, battery)| grid - battery)
}

pub(crate) fn coordinated_w(input: &SafetyInput) -> f64 {
    input
        .coordinated_pack_w
        .iter()
        .copied()
        .filter(|w| w.is_finite())
        .sum()
}

pub(crate) fn projected_import(input: &SafetyInput, self_w: f64) -> Option<f64> {
    fleet_baseline_w(input).map(|baseline| baseline + coordinated_w(input) + self_w)
}

/// Meter, fleet battery and service-rating values must be finite and physically plausible.
/// Import-positive grid power is never negative; zero is a valid no-import reading.
pub(crate) fn valid_service_evidence(input: &SafetyInput) -> bool {
    let grid_ok = input
        .grid_w
        .is_some_and(|grid| grid.is_finite() && (0.0..=MAX_PLAUSIBLE_POWER_W).contains(&grid));
    let battery_ok = input
        .battery_w
        .is_some_and(|battery| battery.is_finite() && battery.abs() <= MAX_PLAUSIBLE_POWER_W);
    let limit = input.physical_service_limit_w;
    let limit_ok = limit.is_finite() && limit > 0.0 && limit <= MAX_PLAUSIBLE_POWER_W;
    grid_ok && battery_ok && limit_ok
}

/// Service-headroom projection exists only with fresh, numerically valid service evidence.
pub(crate) fn service_projection(input: &SafetyInput, self_w: f64) -> Option<f64> {
    if !valid_service_evidence(input)
        || !fresh_enough(input.grid_sample_at_ms, input.at_ms, COMMAND_SAMPLE_MAX_MS)
    {
        return None;
    }
    projected_import(input, self_w)
}

pub(crate) fn cell_hold(input: &SafetyInput) -> Option<Candidate> {
    if input.cell_latch || (input.cell_min_v < CELL_TRIGGER_V && !input.cell_force_charge) {
        return Some(hold(Rule::CellLatch));
    }
    None
}

fn emergency_target_w(input: &SafetyInput) -> f64 {
    let deficit_v = (CELL_RESUME_V - input.cell_min_v).max(0.0);
    let deficit_kwh = input.pack_capacity_kwh * deficit_v / CELL_NOMINAL_V;
    let needed_w = deficit_kwh * 1_000.0 * 3_600_000.0 / f64::from(EMERGENCY_DURATION_MS as u32);
    let aggregate_share = AGGREGATE_EMERGENCY_W / input.fleet_pack_count.max(1) as f64;
    needed_w
        .min(input.max_charge_w)
        .min(aggregate_share)
        .max(0.0)
}

pub(crate) fn cell_emergency(input: &SafetyInput) -> Option<Candidate> {
    if !input.cell_force_charge || input.cell_min_v >= CELL_RESUME_V {
        return None;
    }
    let mut limits = Vec::new();
    if !fresh_enough(
        input.cell_observed_at_ms,
        input.at_ms,
        COMMAND_SAMPLE_MAX_MS,
    ) {
        limits.push("cell_sample_stale");
    }
    if !fresh_enough(
        Some(input.last_sample_at_ms),
        input.at_ms,
        COMMAND_SAMPLE_MAX_MS,
    ) {
        limits.push("pack_sample_stale");
    }
    if input.command_budget_remaining_ms < AUTHORIZATION_MIN_MS as u32 {
        limits.push("observation_budget_exhausted");
    }
    let target_w = emergency_target_w(input);
    limits.extend(physical_limits(input, true, target_w));
    let projected = service_projection(input, target_w);
    if projected.is_none() {
        limits.push("service_headroom_unavailable");
    } else if projected.is_some_and(|grid| grid > input.physical_service_limit_w) {
        limits.push("physical_service_limit");
    }
    if limits.is_empty() {
        let mut result = candidate(
            Rule::CellEmergency,
            Some(SafetyAction::EmergencyCharge { target_w }),
        );
        if projected.is_some_and(|grid| grid > input.import_ceiling_w) {
            result.extra_reasons.push("import_margin_breached");
        }
        return Some(result);
    }
    Some(blocked(
        Rule::CellEmergency,
        "emergency_charge_impossible",
        limits,
    ))
}

pub(crate) fn soc_floor(input: &SafetyInput) -> Option<Candidate> {
    if !input.at_floor() {
        return None;
    }
    let needs_discharge = input.plan_w < 0.0
        || fleet_baseline_w(input)
            .is_some_and(|demand| demand > input.import_ceiling_w + input.import_deadband_w);
    if needs_discharge {
        Some(hold(Rule::SocFloor))
    } else {
        Some(candidate(Rule::SocFloor, None))
    }
}

pub(crate) fn full_charge(input: &SafetyInput) -> Option<Candidate> {
    if input.at_ceiling() && input.plan_w > 0.0 {
        let mut result = hold(Rule::FullCharge);
        result.binding_limits.push("soc_ceiling");
        return Some(result);
    }
    None
}

pub(crate) fn export_guard(input: &SafetyInput) -> Option<Candidate> {
    let grid = input.grid_w?;
    let projected = grid - input.battery_w.unwrap_or(0.0) + input.plan_w;
    if input.plan_w < 0.0 && projected < -input.export_ceiling_w {
        return Some(hold(Rule::ExportGuard));
    }
    None
}

pub(crate) fn stale_pack(input: &SafetyInput) -> Option<Candidate> {
    if input.sample_stale() {
        return Some(hold(Rule::StaleSample));
    }
    None
}

pub(crate) fn observation_budget(input: &SafetyInput, request: &SlotRequest) -> Option<Candidate> {
    if input.command_budget_remaining_ms < AUTHORIZATION_MIN_MS as u32
        && request.requested_w.abs() > HOLD_DEADBAND_W
    {
        return Some(hold(Rule::ObservationBudget));
    }
    None
}

pub(crate) fn orphan_hold(input: &SafetyInput) -> Option<Candidate> {
    if input.ownership_lost {
        return Some(hold(Rule::OrphanHold));
    }
    None
}

pub(crate) fn cancel_ramp(input: &SafetyInput) -> Option<Candidate> {
    if input.cancel_ramp && input.standing_command_w.abs() > HOLD_DEADBAND_W {
        return Some(hold(Rule::CancelRamp));
    }
    None
}

pub(crate) fn restart_grace(input: &SafetyInput, now_ms: i64) -> Option<Candidate> {
    if input
        .restart_at_ms
        .is_some_and(|at| now_ms - at <= RESTART_GRACE_MS)
        && (input.standing_command_w.abs() > HOLD_DEADBAND_W || input.uncertain)
    {
        return Some(hold(Rule::RestartGrace));
    }
    None
}

pub(crate) fn unverified_hold(
    state: &LadderState,
    input: &SafetyInput,
    request: &SlotRequest,
) -> Option<Candidate> {
    if request.safety_traffic {
        return None;
    }
    let unresolved = state
        .holds
        .get(&input.pack_id)
        .is_some_and(HoldProof::blocks_ordinary);
    if unresolved {
        return Some(hold(Rule::UnverifiedHold));
    }
    None
}

pub(crate) fn uncertain(input: &SafetyInput, request: &SlotRequest) -> Option<Candidate> {
    if !input.uncertain {
        return None;
    }
    let action = if request.safety_traffic {
        None
    } else {
        Some(SafetyAction::NeutralHold)
    };
    Some(candidate(Rule::UncertainWrite, action))
}

pub(crate) fn backoff(
    input: &SafetyInput,
    request: &SlotRequest,
    now_ms: i64,
) -> Option<Candidate> {
    if request.safety_traffic || now_ms >= input.backoff_until_ms {
        return None;
    }
    let mut result = candidate(Rule::FaultBackoff, None);
    result.defer_until_ms = Some(input.backoff_until_ms);
    Some(result)
}

pub(crate) fn no_command(input: &SafetyInput, request: &SlotRequest) -> Option<Candidate> {
    if request.requested_w == 0.0 && input.standing_command_w.abs() <= HOLD_DEADBAND_W {
        return Some(candidate(Rule::NoCommand, None));
    }
    None
}
