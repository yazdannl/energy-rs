//! Private M5 decision state and proof bookkeeping.

use std::collections::HashMap;

use crate::dispatch::SafetyAction;
use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::safety::{
    COMMAND_SAMPLE_MAX_MS, HoldProof, Rule, SafetyDecision, SafetyInput, UNOBSERVED_BUDGET_MS,
    fresh_enough,
};

#[derive(Debug, Clone)]
pub(crate) struct ImportLatch {
    pub armed: bool,
    pub ask_w: f64,
    pub at_ms: i64,
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)] // units are part of the budget contract
pub(crate) struct ObservationBudget {
    pub remaining_ms: u32,
    pub last_eval_at_ms: i64,
    pub last_sample_seen_at_ms: Option<i64>,
}

impl ObservationBudget {
    pub fn new(at_ms: i64) -> Self {
        Self {
            remaining_ms: UNOBSERVED_BUDGET_MS as u32,
            last_eval_at_ms: at_ms,
            last_sample_seen_at_ms: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LadderState {
    pub(crate) inputs: HashMap<String, SafetyInput>,
    pub(crate) decisions: Vec<SafetyDecision>,
    pub(crate) holds: HashMap<String, HoldProof>,
    pub(crate) imports: HashMap<String, ImportLatch>,
    pub(crate) budgets: HashMap<String, ObservationBudget>,
    pub(crate) seq: u64,
}

pub(crate) fn observe_budget(state: &mut LadderState, input: &SafetyInput, now_ms: i64) -> u32 {
    let sample = input.last_sample_at_ms;
    let budget = match state.budgets.entry(input.pack_id.clone()) {
        std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::hash_map::Entry::Vacant(entry) => {
            let mut budget = ObservationBudget::new(now_ms);
            if fresh_enough(Some(sample), now_ms, COMMAND_SAMPLE_MAX_MS) {
                budget.last_sample_seen_at_ms = Some(sample);
            }
            entry.insert(budget)
        }
    };
    if now_ms <= budget.last_eval_at_ms {
        return budget.remaining_ms;
    }
    let elapsed = now_ms.saturating_sub(budget.last_eval_at_ms);
    let fresh = fresh_enough(Some(sample), now_ms, COMMAND_SAMPLE_MAX_MS)
        && budget
            .last_sample_seen_at_ms
            .is_none_or(|seen| sample > seen);
    if fresh {
        let credited_from = budget.last_eval_at_ms.max(sample);
        let credit = u32::try_from(now_ms.saturating_sub(credited_from)).unwrap_or(u32::MAX);
        budget.remaining_ms = budget
            .remaining_ms
            .saturating_add(credit)
            .min(UNOBSERVED_BUDGET_MS as u32);
        budget.last_sample_seen_at_ms = Some(sample);
    } else {
        budget.remaining_ms = budget
            .remaining_ms
            .saturating_sub(u32::try_from(elapsed).unwrap_or(u32::MAX));
    }
    budget.last_eval_at_ms = now_ms;
    budget.remaining_ms
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
    let mut prior_escalation = Vec::new();
    if let Some(mut current) = existing
        && current.blocks_ordinary()
    {
        if (current.target_w - target_w).abs() <= HOLD_DEADBAND_W {
            return current;
        }
        prior_escalation.clone_from(&current.escalation.steps);
        current.supersede(rule.reason());
        state.holds.insert(input.pack_id.clone(), current);
    }
    state.seq = state.seq.saturating_add(1);
    let mut proof = HoldProof::new(state.seq, at_ms, target_w, duration_ms);
    proof.prior_escalation = prior_escalation;
    proof.steps.push("target_proof_replaced");
    if rule == Rule::CellEmergency {
        proof.escalation.path = "emergency_charge_unresolved";
    }
    state.holds.insert(input.pack_id.clone(), proof.clone());
    proof
}
