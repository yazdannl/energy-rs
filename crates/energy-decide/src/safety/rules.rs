//! Per-rule hazard predicates. The ordered evaluation lives in `ladder.rs`.

use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::dispatch::{SafetyAction, SlotRequest};
use crate::safety::ladder::ImportLatch;
use crate::safety::ladder::LadderState;
use crate::safety::{
    CELL_RESUME_V, CELL_TRIGGER_V, EMERGENCY_CHARGE_W, RESTART_GRACE_MS, Rule, SafetyInput,
};

/// One fired rule, its reason and the command it selects.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub rule: Rule,
    pub reason: &'static str,
    pub action: Option<SafetyAction>,
    pub binding_limits: Vec<&'static str>,
    pub blocked: bool,
}

fn candidate(rule: Rule, action: Option<SafetyAction>) -> Candidate {
    Candidate {
        rule,
        reason: rule.reason(),
        action,
        binding_limits: Vec::new(),
        blocked: false,
    }
}

fn blocked(rule: Rule, reason: &'static str, limits: Vec<&'static str>) -> Candidate {
    Candidate {
        rule,
        reason,
        action: Some(SafetyAction::NeutralHold),
        binding_limits: limits,
        blocked: true,
    }
}

fn hold(rule: Rule) -> Candidate {
    candidate(rule, Some(SafetyAction::NeutralHold))
}

fn physical_limits(input: &SafetyInput, charge: bool, required_w: f64) -> Vec<&'static str> {
    let mut limits = Vec::new();
    if !input.temperature_ok {
        limits.push("temperature_range");
    }
    if !input.current_limit_ok {
        limits.push("capacity_current_limit");
    }
    if charge {
        if input.max_charge_w <= 0.0 || required_w > input.max_charge_w {
            limits.push("max_charge_w");
        }
        if input.at_ceiling() {
            limits.push("soc_ceiling");
        }
    } else if input.max_discharge_w <= 0.0 || required_w.abs() > input.max_discharge_w {
        limits.push("max_discharge_w");
    }
    if !charge && input.at_floor() {
        limits.push("soc_floor");
    }
    limits
}

pub(crate) fn cell_hold(input: &SafetyInput) -> Option<Candidate> {
    if input.cell_latch || (input.cell_min_v < CELL_TRIGGER_V && !input.cell_force_charge) {
        return Some(hold(Rule::CellLatch));
    }
    None
}

pub(crate) fn cell_emergency(input: &SafetyInput) -> Option<Candidate> {
    if !input.cell_force_charge || input.cell_stale() || input.cell_min_v >= CELL_RESUME_V {
        return None;
    }
    let target_w = EMERGENCY_CHARGE_W.max(input.plan_w);
    let limits = physical_limits(input, true, target_w);
    if limits.is_empty() {
        return Some(candidate(
            Rule::CellEmergency,
            Some(SafetyAction::EmergencyCharge { target_w }),
        ));
    }
    Some(blocked(
        Rule::CellEmergency,
        "emergency_charge_impossible",
        limits,
    ))
}

fn demand_w(input: &SafetyInput) -> Option<f64> {
    input
        .grid_w
        .zip(input.battery_w)
        .map(|(grid, battery)| grid - battery)
}

pub(crate) fn soc_floor(input: &SafetyInput) -> Option<Candidate> {
    if !input.at_floor() {
        return None;
    }
    let needs_discharge = input.plan_w < 0.0
        || demand_w(input)
            .is_some_and(|demand| demand > input.import_ceiling_w + input.import_deadband_w);
    if needs_discharge {
        Some(hold(Rule::SocFloor))
    } else {
        Some(candidate(Rule::SocFloor, None))
    }
}

pub(crate) fn import_ceiling(
    input: &SafetyInput,
    state: &mut LadderState,
    now_ms: i64,
) -> Option<Candidate> {
    let latch = state
        .imports
        .entry(input.pack_id.clone())
        .or_insert(ImportLatch {
            armed: input.import_armed,
            ask_w: 0.0,
            at_ms: now_ms,
        });
    let demand = demand_w(input);
    if let Some(demand) = demand
        && demand > input.import_ceiling_w + input.import_deadband_w
    {
        latch.armed = true;
        latch.ask_w = input.import_ceiling_w - demand;
        latch.at_ms = now_ms;
    }
    let released = input
        .settled_grid_w
        .is_some_and(|grid| grid <= input.import_ceiling_w - input.import_deadband_w);
    if released {
        latch.armed = false;
        return None;
    }
    if !latch.armed && input.grid_w.is_none() {
        let mut item = hold(Rule::ImportCeiling);
        item.reason = "grid_telemetry_missing";
        return Some(item);
    }
    if !latch.armed {
        return None;
    }
    let mut result = candidate(
        Rule::ImportCeiling,
        Some(SafetyAction::ProtectiveDischarge {
            target_w: latch.ask_w,
        }),
    );
    let limits = physical_limits(input, false, latch.ask_w);
    if !limits.is_empty() {
        let floor_only = limits.len() == 1 && limits[0] == "soc_floor";
        let reason = if floor_only {
            Rule::ImportCeiling.reason()
        } else {
            "import_remedy_blocked"
        };
        result = blocked(Rule::ImportCeiling, reason, limits);
    }
    if input.grid_w.is_none() {
        result.reason = "grid_telemetry_missing";
    }
    Some(result)
}

pub(crate) fn full_charge(input: &SafetyInput) -> Option<Candidate> {
    if input.at_ceiling() && input.plan_w > 0.0 {
        let mut result = candidate(Rule::FullCharge, Some(SafetyAction::NeutralHold));
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

pub(crate) fn transition(input: &SafetyInput, now_ms: i64) -> Vec<Candidate> {
    let mut found = Vec::new();
    if input.sample_stale() {
        found.push(hold(Rule::StaleSample));
    }
    if input.ownership_lost {
        found.push(hold(Rule::OrphanHold));
    }
    if input.cancel_ramp && input.standing_command_w.abs() > HOLD_DEADBAND_W {
        found.push(hold(Rule::CancelRamp));
    }
    if input
        .restart_at_ms
        .is_some_and(|at| now_ms - at <= RESTART_GRACE_MS)
        && (input.standing_command_w.abs() > HOLD_DEADBAND_W || input.uncertain)
    {
        found.push(hold(Rule::RestartGrace));
    }
    found
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
    Some(candidate(Rule::FaultBackoff, None))
}

pub(crate) fn no_command(input: &SafetyInput, request: &SlotRequest) -> Option<Candidate> {
    if request.requested_w == 0.0 && input.standing_command_w.abs() <= HOLD_DEADBAND_W {
        return Some(candidate(Rule::NoCommand, None));
    }
    None
}
