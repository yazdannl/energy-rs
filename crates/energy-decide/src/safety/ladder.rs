//! M5 safety ladder: one ordered rule list, verified holds, bounded escalation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::dispatch::{SafetyAction, SafetyGate, SafetyIntervention, SlotRequest};
use crate::safety::rules::{
    Candidate, backoff, cell_emergency, cell_hold, export_guard, full_charge, import_ceiling,
    no_command, soc_floor, transition, uncertain,
};
use crate::safety::{
    DecisionOutcome, Escalation, HoldProof, SafetyDecision, SafetyInput, SafetyReceipt,
    SolarHeadroom,
};

/// Cell guard thresholds shipped by `0.15.9:.../protection/cell_config.rs`.
pub const CELL_TRIGGER_V: f64 = 3.00;
/// Resume threshold for a latched emergency charge.
pub const CELL_RESUME_V: f64 = 3.18;
/// Shipped emergency-charge default, W.
pub const EMERGENCY_CHARGE_W: f64 = 800.0;
/// Restart grace window from SPEC M5, milliseconds.
pub const RESTART_GRACE_MS: i64 = 120_000;

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

/// Pure decision core for the safety ladder. It never writes and never waits.
#[derive(Debug, Default)]
pub struct SafetyLadder {
    state: Mutex<LadderState>,
}

impl SafetyLadder {
    /// Fresh ladder with no hazards and no decisions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shareable ladder for dispatch and telemetry observers.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Store the newest hazard snapshot for one pack.
    pub fn observe(&self, input: SafetyInput) {
        let mut state = self.lock();
        state.inputs.insert(input.pack_id.clone(), input);
    }

    /// Fold a correlated receipt into the open hold proof; echo alone cannot verify it.
    pub fn observe_hold(&self, receipt: &SafetyReceipt, now_ms: i64) {
        let mut state = self.lock();
        if let Some(proof) = state.holds.get_mut(&receipt.pack_id) {
            proof.observe_receipt(receipt, now_ms);
            if proof.verified {
                note(
                    &mut state,
                    &receipt.pack_id,
                    "hold_verified_by_measured_flow",
                );
            }
        }
    }

    /// Record a failed safety attempt and its next bounded retry.
    pub fn fail_hold(&self, pack_id: &str, at_ms: i64, reason: &'static str) {
        let mut state = self.lock();
        if let Some(proof) = state.holds.get_mut(pack_id) {
            proof.fail(at_ms, reason);
            note(&mut state, pack_id, reason);
        }
    }

    /// Last decision recorded for one pack, including explicit no-action decisions.
    pub fn last_decision(&self, pack_id: &str) -> Option<SafetyDecision> {
        self.lock()
            .decisions
            .iter()
            .rev()
            .find(|decision| decision.pack_id == pack_id)
            .cloned()
    }

    /// All decisions in evaluation order, for reporting and tests.
    pub fn decisions(&self) -> Vec<SafetyDecision> {
        self.lock().decisions.clone()
    }

    /// Current hold proof for one pack, when a safety hold was issued.
    pub fn hold_proof(&self, pack_id: &str) -> Option<HoldProof> {
        self.lock().holds.get(pack_id).cloned()
    }

    /// Evaluate one request. This is synchronous: no hold is ever awaited in a tick.
    pub fn decide(&self, request: &SlotRequest, now_ms: i64) -> SafetyDecision {
        let mut state = self.lock();
        let mut input = state
            .inputs
            .get(&request.pack_id)
            .cloned()
            .unwrap_or_else(|| SafetyInput::new(&request.pack_id, now_ms));
        input.at_ms = now_ms;
        input.plan_w = request.requested_w;
        state.inputs.insert(input.pack_id.clone(), input.clone());
        let solar = SolarHeadroom::evaluate(&input);
        let candidates = evaluate(&input, request, &mut state, now_ms);
        let mut decision = build(&input, &candidates, solar, now_ms);
        if decision.action == Some(SafetyAction::NeutralHold) {
            let proof = hold(&mut state, &input.pack_id, now_ms);
            decision.retry_at_ms = proof.escalation.retry_at_ms;
            decision.steps.extend(proof.steps);
        }
        state.decisions.push(decision.clone());
        decision
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LadderState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SafetyGate for SafetyLadder {
    fn preempt(&self, request: &SlotRequest, now_ms: i64) -> SafetyIntervention {
        let decision = self.decide(request, now_ms);
        match decision.action {
            Some(action) => SafetyIntervention::Hold {
                action,
                reason: decision.reason,
            },
            None => SafetyIntervention::Proceed,
        }
    }
}

fn note(state: &mut LadderState, pack_id: &str, step: &'static str) {
    if let Some(decision) = state
        .decisions
        .iter_mut()
        .rev()
        .find(|d| d.pack_id == pack_id)
    {
        decision.steps.push(step);
    }
}

fn hold(state: &mut LadderState, pack_id: &str, at_ms: i64) -> HoldProof {
    let fresh = state
        .holds
        .get(pack_id)
        .is_none_or(|proof| proof.verified || proof.escalation.exhausted());
    if fresh {
        state.seq = state.seq.saturating_add(1);
        let proof = HoldProof::new(state.seq, at_ms, 0.0);
        state.holds.insert(pack_id.to_owned(), proof);
    }
    state
        .holds
        .get(pack_id)
        .cloned()
        .unwrap_or_else(|| HoldProof::new(0, at_ms, 0.0))
}

fn evaluate(
    input: &SafetyInput,
    request: &SlotRequest,
    state: &mut LadderState,
    now_ms: i64,
) -> Vec<Candidate> {
    let mut found = Vec::new();
    push_if(cell_hold(input), &mut found);
    push_if(cell_emergency(input), &mut found);
    push_if(soc_floor(input), &mut found);
    push_if(import_ceiling(input, state, now_ms), &mut found);
    push_if(full_charge(input), &mut found);
    push_if(export_guard(input), &mut found);
    found.extend(transition(input, now_ms));
    push_if(uncertain(input, request), &mut found);
    push_if(backoff(input, request, now_ms), &mut found);
    push_if(no_command(input, request), &mut found);
    found
}

fn push_if(result: Option<Candidate>, found: &mut Vec<Candidate>) {
    if let Some(result) = result {
        found.push(result);
    }
}

fn build(
    input: &SafetyInput,
    candidates: &[Candidate],
    solar: SolarHeadroom,
    now_ms: i64,
) -> SafetyDecision {
    let mut ordered: Vec<&Candidate> = candidates.iter().collect();
    ordered.sort_by_key(|item| item.rule.position());
    let reasons: Vec<&'static str> = ordered.iter().map(|item| item.reason).collect();
    let Some(winner) = ordered.iter().find(|item| item.action.is_some()) else {
        let mut reasons = reasons;
        reasons.push(solar.reason);
        return SafetyDecision::no_action(
            &input.pack_id,
            now_ms,
            solar,
            input.targets.effective_kwh(),
            input.targets.soft_guideline_kwh,
            reasons,
        );
    };
    let command = winner.action.map_or(0.0, SafetyAction::target_w);
    let mut steps = vec!["rules_evaluated", winner.reason];
    steps.extend(winner.binding_limits.iter().copied());
    steps.push(solar.reason);
    SafetyDecision {
        pack_id: input.pack_id.clone(),
        at_ms: now_ms,
        rule: winner.rule,
        outcome: if winner.blocked {
            DecisionOutcome::Blocked
        } else {
            DecisionOutcome::Commanded
        },
        action: winner.action,
        command_w: command,
        reason: winner.reason,
        reasons,
        binding_limits: winner.binding_limits.clone(),
        effective_target_kwh: input.targets.effective_kwh(),
        raw_guideline_kwh: input.targets.soft_guideline_kwh,
        solar,
        verification: winner.rule.verification(),
        escalation: Some(Escalation::new(winner.rule.escalation())),
        steps,
        retry_at_ms: None,
    }
}
