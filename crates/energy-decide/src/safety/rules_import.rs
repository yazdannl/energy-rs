//! Projected-import ceiling rule, convergent remedy and the settled-evidence latch.

use crate::dispatch::SafetyAction;
use crate::safety::rules::{
    Candidate, blocked, candidate, coordinated_w, fleet_baseline_w, hold, projected_import,
};
use crate::safety::rules_limits::physical_limits;
use crate::safety::solar::TELEMETRY_FRESH_MS;
use crate::safety::state::{ImportLatch, LadderState};
use crate::safety::{
    AUTHORIZATION_MIN_MS, IMPORT_CONVERGENCE_MARGIN_W, Rule, SafetyInput, fresh_enough,
};

/// Import target that a successful remedy must reach before the latch can release.
pub(crate) fn release_target_w(input: &SafetyInput) -> f64 {
    input.import_ceiling_w - input.import_deadband_w - IMPORT_CONVERGENCE_MARGIN_W
}

/// Why this snapshot cannot command a remedy that converges below the release threshold.
pub(crate) fn remedy_unavailable(input: &SafetyInput, ask_w: f64) -> Option<&'static str> {
    if fleet_baseline_w(input).is_none() {
        return Some("grid_telemetry_missing");
    }
    physical_limits(input, false, ask_w).into_iter().next()
}

pub(crate) fn import_guard_active(state: &LadderState, pack_id: &str) -> bool {
    state.imports.get(pack_id).is_some_and(|latch| latch.armed)
}

/// True only while the currently evaluated snapshot is actually breaching.
pub(crate) fn import_breach_active(input: &SafetyInput) -> bool {
    projected_import(input, input.plan_w)
        .is_some_and(|grid| grid > input.import_ceiling_w + input.import_deadband_w)
}

fn update_latch(input: &SafetyInput, state: &mut LadderState, now_ms: i64) -> bool {
    let latch = state
        .imports
        .entry(input.pack_id.clone())
        .or_insert(ImportLatch {
            armed: input.import_armed,
            ask_w: 0.0,
            at_ms: now_ms,
            unavailable_reason: None,
        });
    latch.armed = latch.armed || input.import_armed;
    let breach = import_breach_active(input);
    if breach {
        let baseline = fleet_baseline_w(input).unwrap_or_default();
        latch.ask_w = (release_target_w(input) - baseline - coordinated_w(input)).min(0.0);
        latch.unavailable_reason = remedy_unavailable(input, latch.ask_w);
        latch.armed = true;
        latch.at_ms = now_ms;
    }
    breach
}

fn fresh_settled(input: &SafetyInput, state: &LadderState, now_ms: i64) -> bool {
    let fallback = ImportLatch {
        armed: input.import_armed,
        ask_w: 0.0,
        at_ms: now_ms,
        unavailable_reason: None,
    };
    let latch = state.imports.get(&input.pack_id).unwrap_or(&fallback);
    input
        .settled_grid_w
        .zip(input.settled_at_ms)
        .is_some_and(|(grid, at)| {
            at > latch.at_ms
                && fresh_enough(Some(at), now_ms, TELEMETRY_FRESH_MS)
                && grid <= input.import_ceiling_w - input.import_deadband_w
        })
}

fn remedy(input: &SafetyInput, state: &LadderState, now_ms: i64) -> Option<Candidate> {
    let latch = state.imports.get(&input.pack_id)?;
    let ask_w = latch.ask_w;
    let mut result = if ask_w < 0.0 {
        candidate(
            Rule::ImportCeiling,
            Some(SafetyAction::ProtectiveDischarge { target_w: ask_w }),
        )
    } else {
        hold(Rule::ImportCeiling)
    };
    let mut limits = physical_limits(input, false, ask_w);
    if let Some(reason) = latch.unavailable_reason {
        if !limits.contains(&reason) {
            limits.push(reason);
        }
        result.reason = "import_remedy_unavailable";
    }
    if ask_w < 0.0 && input.command_budget_remaining_ms < AUTHORIZATION_MIN_MS as u32 {
        limits.push("observation_budget_exhausted");
    }
    if !fresh_settled(input, state, now_ms) {
        result.extra_reasons.push("settled_grid_evidence_missing");
    }
    if !limits.is_empty() {
        let floor_only = limits.len() == 1 && limits[0] == "soc_floor";
        let reason = if floor_only {
            Rule::ImportCeiling.reason()
        } else {
            "import_remedy_blocked"
        };
        result = blocked(Rule::ImportCeiling, reason, limits);
        if latch.unavailable_reason.is_some() {
            result.reason = "import_remedy_unavailable";
        }
        if !fresh_settled(input, state, now_ms) {
            result.extra_reasons.push("settled_grid_evidence_missing");
        }
    }
    if input.grid_w.is_none() {
        result.reason = "grid_telemetry_missing";
    }
    Some(result)
}

pub(crate) fn import_ceiling(
    input: &SafetyInput,
    state: &mut LadderState,
    now_ms: i64,
) -> Option<Candidate> {
    let breach = update_latch(input, state, now_ms);
    let fresh = fresh_settled(input, state, now_ms);
    if !breach && fresh {
        if let Some(latch) = state.imports.get_mut(&input.pack_id) {
            latch.armed = false;
            latch.ask_w = 0.0;
            latch.unavailable_reason = None;
        }
        return None;
    }
    if !breach && !import_guard_active(state, &input.pack_id) {
        return input.grid_w.is_none().then(|| {
            let mut item = hold(Rule::ImportCeiling);
            item.reason = "grid_telemetry_missing";
            item
        });
    }
    remedy(input, state, now_ms)
}
