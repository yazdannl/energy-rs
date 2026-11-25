//! Projected-import ceiling rule and its settled-evidence latch.

use crate::dispatch::SafetyAction;
use crate::safety::rules::{
    Candidate, blocked, candidate, coordinated_w, fleet_baseline_w, hold, projected_import,
};
use crate::safety::rules_limits::physical_limits;
use crate::safety::solar::TELEMETRY_FRESH_MS;
use crate::safety::state::{ImportLatch, LadderState};
use crate::safety::{AUTHORIZATION_MIN_MS, Rule, SafetyInput, fresh_enough};

fn update_latch(input: &SafetyInput, state: &mut LadderState, now_ms: i64) -> bool {
    let latch = state
        .imports
        .entry(input.pack_id.clone())
        .or_insert(ImportLatch {
            armed: input.import_armed,
            ask_w: 0.0,
            at_ms: now_ms,
        });
    latch.armed = latch.armed || input.import_armed;
    let projected = projected_import(input, input.plan_w);
    let breach =
        projected.is_some_and(|grid| grid > input.import_ceiling_w + input.import_deadband_w);
    if breach {
        let headroom = fleet_baseline_w(input).map_or(0.0, |baseline| {
            input.import_ceiling_w - baseline - coordinated_w(input)
        });
        latch.armed = true;
        latch.ask_w = headroom.min(0.0);
        latch.at_ms = now_ms;
    }
    breach
}

fn fresh_settled(input: &SafetyInput, state: &LadderState, now_ms: i64) -> bool {
    let fallback = ImportLatch {
        armed: input.import_armed,
        ask_w: 0.0,
        at_ms: now_ms,
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

fn remedy(input: &SafetyInput, state: &LadderState) -> Option<Candidate> {
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
    if ask_w < 0.0 && input.command_budget_remaining_ms < AUTHORIZATION_MIN_MS as u32 {
        limits.push("observation_budget_exhausted");
    }
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
        }
        return None;
    }
    if !breach
        && !state
            .imports
            .get(&input.pack_id)
            .is_some_and(|latch| latch.armed)
    {
        return input.grid_w.is_none().then(|| {
            let mut item = hold(Rule::ImportCeiling);
            item.reason = "grid_telemetry_missing";
            item
        });
    }
    remedy(input, state)
}
