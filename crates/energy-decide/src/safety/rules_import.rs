//! Projected-import ceiling rule with four independent computations.
//!
//! `breach_now` is measured grid import only and arms the latch. `proposal_safe` is a projection
//! and decides command execution. `protective_command` is computed from the current snapshot, not
//! from the latch. The latch only records the measured-breach episode and its settled release.

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

/// Import target that a protective command must reach before the latch can release.
pub(crate) fn release_target_w(input: &SafetyInput) -> f64 {
    input.import_ceiling_w - input.import_deadband_w - IMPORT_CONVERGENCE_MARGIN_W
}

/// Threshold that settled measured import must meet to prove the breach was remedied.
pub(crate) fn release_threshold_w(input: &SafetyInput) -> f64 {
    input.import_ceiling_w - input.import_deadband_w
}

/// 1. Measured grid import above the ceiling: the only latch-arming event.
pub(crate) fn breach_now(input: &SafetyInput) -> bool {
    input
        .grid_w
        .is_some_and(|grid| grid.is_finite() && grid > input.import_ceiling_w)
}

/// 2. Projected import if the proposed economic plan were executed: command safety only.
pub(crate) fn proposal_safe(input: &SafetyInput) -> bool {
    projected_import(input, input.plan_w)
        .is_some_and(|grid| grid.is_finite() && grid <= input.import_ceiling_w)
}

/// 3. Protective command for an unsafe proposal, computed without latch state.
pub(crate) fn protective_command_w(input: &SafetyInput) -> f64 {
    let baseline = fleet_baseline_w(input).unwrap_or_default();
    (release_target_w(input) - baseline - coordinated_w(input)).min(0.0)
}

/// Settled measured import inside the release band for one episode window.
fn settled_remedied_from(input: &SafetyInput, episode_start: i64, now_ms: i64) -> bool {
    input
        .settled_grid_w
        .zip(input.settled_at_ms)
        .is_some_and(|(grid, at)| {
            grid.is_finite()
                && at > episode_start
                && fresh_enough(Some(at), now_ms, TELEMETRY_FRESH_MS)
                && grid <= release_threshold_w(input)
        })
}

/// 4a. Settled measured import inside the release band, newer than this breach episode.
pub(crate) fn breach_remedied(input: &SafetyInput, state: &LadderState, now_ms: i64) -> bool {
    let episode_start = state
        .imports
        .get(&input.pack_id)
        .map_or(now_ms, |latch| latch.at_ms);
    settled_remedied_from(input, episode_start, now_ms)
}

pub(crate) fn import_guard_active(state: &LadderState, pack_id: &str) -> bool {
    state.imports.get(pack_id).is_some_and(|latch| latch.armed)
}

pub(crate) fn import_episode_start_ms(state: &LadderState, pack_id: &str) -> i64 {
    state.imports.get(pack_id).map_or(0, |latch| latch.at_ms)
}

/// Arm only on a new measured breach; release only on settled measured remediation.
fn update_latch(input: &SafetyInput, state: &mut LadderState, now_ms: i64) {
    let current = state
        .imports
        .get(&input.pack_id)
        .cloned()
        .unwrap_or(ImportLatch {
            armed: false,
            at_ms: now_ms,
        });
    let measured_breach = breach_now(input);
    let episode_start = if measured_breach {
        now_ms
    } else {
        current.at_ms
    };
    let mut armed = current.armed || measured_breach;
    if armed && settled_remedied_from(input, episode_start, now_ms) {
        armed = false;
    }
    state.imports.insert(
        input.pack_id.clone(),
        ImportLatch {
            armed,
            at_ms: episode_start,
        },
    );
}

/// A protective command that cannot converge must say so explicitly.
fn protective_candidate(input: &SafetyInput, now_ms: i64) -> Candidate {
    let ask_w = protective_command_w(input);
    let mut result = if ask_w < 0.0 {
        candidate(
            Rule::ImportCeiling,
            Some(SafetyAction::ProtectiveDischarge { target_w: ask_w }),
        )
    } else {
        hold(Rule::ImportCeiling)
    };
    let unavailable = fleet_baseline_w(input)
        .is_none()
        .then_some("grid_telemetry_missing")
        .or_else(|| physical_limits(input, false, ask_w).into_iter().next());
    let mut limits = physical_limits(input, false, ask_w);
    if let Some(reason) = unavailable {
        if !limits.contains(&reason) {
            limits.push(reason);
        }
        result.reason = "import_remedy_unavailable";
    }
    if ask_w < 0.0 && input.command_budget_remaining_ms < AUTHORIZATION_MIN_MS as u32 {
        limits.push("observation_budget_exhausted");
    }
    if !breach_remedied_against(input, now_ms) {
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
        if unavailable.is_some() {
            result.reason = "import_remedy_unavailable";
        }
        if !breach_remedied_against(input, now_ms) {
            result.extra_reasons.push("settled_grid_evidence_missing");
        }
    }
    if input.grid_w.is_none() {
        result.reason = "grid_telemetry_missing";
    }
    result
}

/// Evidence freshness for command reporting; the latch uses its own episode window.
fn breach_remedied_against(input: &SafetyInput, now_ms: i64) -> bool {
    input
        .settled_grid_w
        .zip(input.settled_at_ms)
        .is_some_and(|(grid, at)| {
            grid.is_finite()
                && fresh_enough(Some(at), now_ms, TELEMETRY_FRESH_MS)
                && grid <= release_threshold_w(input)
        })
}

pub(crate) fn import_ceiling(
    input: &SafetyInput,
    state: &mut LadderState,
    now_ms: i64,
) -> Option<Candidate> {
    update_latch(input, state, now_ms);
    // command = if !proposal_safe { protective_command } else { plan }
    if !proposal_safe(input) {
        return Some(protective_candidate(input, now_ms));
    }
    input.grid_w.is_none().then(|| {
        let mut item = hold(Rule::ImportCeiling);
        item.reason = "grid_telemetry_missing";
        item
    })
}
