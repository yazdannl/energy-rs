//! Private M5 decision state and proof bookkeeping.

use std::collections::HashMap;

use crate::dispatch::SafetyAction;
use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::safety::{HoldProof, Rule, SafetyDecision, SafetyInput};

#[derive(Debug, Clone)]
pub(crate) struct ImportLatch {
    pub armed: bool,
    pub ask_w: f64,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LadderState {
    pub(crate) inputs: HashMap<String, SafetyInput>,
    pub(crate) decisions: Vec<SafetyDecision>,
    pub(crate) holds: HashMap<String, HoldProof>,
    pub(crate) imports: HashMap<String, ImportLatch>,
    pub(crate) seq: u64,
}

pub(crate) fn note(state: &mut LadderState, pack_id: &str, step: &'static str) {
    if let Some(decision) = state
        .decisions
        .iter_mut()
        .rev()
        .find(|decision| decision.pack_id == pack_id)
    {
        decision.steps.push(step);
    }
}

pub(crate) fn advance_due(state: &mut LadderState, now_ms: i64) {
    let mut notes = Vec::new();
    for (pack_id, proof) in &mut state.holds {
        if let Some(step) = proof.advance_due(now_ms) {
            notes.push((pack_id.clone(), step));
        }
    }
    for (pack_id, step) in notes {
        note(state, &pack_id, step);
    }
}

pub(crate) fn proof_for(
    state: &mut LadderState,
    input: &SafetyInput,
    rule: Rule,
    action: SafetyAction,
    at_ms: i64,
    duration_ms: i64,
) -> HoldProof {
    let target_w = action.target_w();
    let existing = state.holds.get(&input.pack_id).cloned();
    if let Some(mut current) = existing
        && current.blocks_ordinary()
    {
        if (current.target_w - target_w).abs() <= HOLD_DEADBAND_W || target_w == 0.0 {
            return current;
        }
        current.supersede(rule.reason());
        state.holds.insert(input.pack_id.clone(), current);
    }
    state.seq = state.seq.saturating_add(1);
    let mut proof = HoldProof::new(state.seq, at_ms, target_w, duration_ms);
    if rule == Rule::CellEmergency {
        proof.escalation.path = "emergency_charge_unresolved";
    }
    state.holds.insert(input.pack_id.clone(), proof.clone());
    proof
}
