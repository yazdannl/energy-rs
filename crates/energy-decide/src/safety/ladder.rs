//! M5 safety ladder: one ordered rule list, verified holds, bounded escalation.

use std::sync::{Arc, Mutex};

use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::dispatch::{SafetyAction, SafetyGate, SafetyIntervention, SlotRequest};
use crate::safety::escalation::SAFETY_ATTEMPT_DEADLINE_MS;
use crate::safety::rules::{
    Candidate, backoff, cancel_ramp, cell_emergency, cell_hold, export_guard, full_charge,
    no_command, observation_budget, orphan_hold, restart_grace, soc_floor, stale_pack, uncertain,
    unverified_hold,
};
use crate::safety::rules_import::import_ceiling;
use crate::safety::rules_service::physical_service;
use crate::safety::state::{LadderState, advance_due, note, observe_budget, proof_for};
use crate::safety::verification::HoldAttemptState;
use crate::safety::{
    DecisionOutcome, Escalation, HoldProof, Rule, SafetyDecision, SafetyInput, SafetyReceipt,
    SolarHeadroom,
};

/// Cell guard thresholds shipped by `0.15.9:.../protection/cell_config.rs`.
pub const CELL_TRIGGER_V: f64 = 3.00;
/// Resume threshold for a latched emergency charge.
pub const CELL_RESUME_V: f64 = 3.18;
/// Shipped emergency-charge default, retained for reporting compatibility.
pub const EMERGENCY_CHARGE_W: f64 = 800.0;
/// Restart grace window from SPEC M5, milliseconds.
pub const RESTART_GRACE_MS: i64 = 120_000;
/// Maximum emergency-charge duration before escalation, milliseconds.
pub const EMERGENCY_DURATION_MS: i64 = 180_000;
/// Nominal cell voltage used to convert a voltage deficit into stored energy, V.
pub const CELL_NOMINAL_V: f64 = 3.7;
/// Aggregate emergency-charge bound; divided over the coordinated fleet.
pub const AGGREGATE_EMERGENCY_W: f64 = crate::FLEET_POWER_W;

/// Pure decision core for the safety ladder. It never writes and never waits.
#[derive(Debug, Default)]
pub struct SafetyLadder {
    pub(crate) state: Mutex<LadderState>,
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

    /// Fold a correlated receipt into the open proof; echo alone cannot verify it.
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

    /// Current proof for one pack, when a safety command was issued.
    pub fn hold_proof(&self, pack_id: &str) -> Option<HoldProof> {
        self.lock().holds.get(pack_id).cloned()
    }

    /// Observation-budget credit still held for one pack, in milliseconds.
    pub fn observation_budget_remaining_ms(&self, pack_id: &str) -> u32 {
        self.lock()
            .budgets
            .get(pack_id)
            .map_or(0, |budget| budget.remaining_ms)
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
        input.command_budget_remaining_ms = observe_budget(&mut state, &input, now_ms);
        state.inputs.insert(input.pack_id.clone(), input.clone());
        advance_due(&mut state, now_ms);
        let solar = SolarHeadroom::evaluate(&input);
        let candidates = evaluate(&input, request, &mut state, now_ms);
        let mut decision = build(&input, &candidates, solar, now_ms);
        if let Some(action) = decision.action {
            let terminal_refused = state.holds.get(&input.pack_id).is_some_and(|proof| {
                proof.attempt_state == HoldAttemptState::Terminal
                    && proof.superseded_by.is_none()
                    && (proof.target_w - action.target_w()).abs() > HOLD_DEADBAND_W
            });
            let duration = if decision.rule == Rule::CellEmergency {
                EMERGENCY_DURATION_MS
            } else {
                i64::try_from(SAFETY_ATTEMPT_DEADLINE_MS).unwrap_or(i64::MAX)
            };
            let proof = proof_for(&mut state, &input, decision.rule, action, now_ms, duration);
            decision.retry_at_ms = proof
                .escalation
                .retry_at_ms
                .or_else(|| (!proof.verified).then(|| proof.deadline_ms()));
            decision.steps.push("hold_proof_attached");
            decision
                .steps
                .extend(proof.escalation.steps.iter().copied());
            if proof.escalation.exhausted() && !proof.verified {
                decision.outcome = DecisionOutcome::Failed;
                decision.steps.push("ownership_retention");
            }
            if terminal_refused {
                decision.action = Some(SafetyAction::NeutralHold);
                decision.command_w = 0.0;
                decision.reason = "terminal_target_change_refused";
                decision.reasons.push("terminal_target_change_refused");
                decision.binding_limits.push("terminal_hold_retained");
                decision.outcome = DecisionOutcome::Failed;
                decision.steps.push("terminal_target_change_refused");
            }
        }
        state.decisions.push(decision.clone());
        decision
    }

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, LadderState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SafetyGate for SafetyLadder {
    fn preempt(&self, request: &SlotRequest, now_ms: i64) -> SafetyIntervention {
        let decision = self.decide(request, now_ms);
        if decision.outcome == DecisionOutcome::Deferred {
            return SafetyIntervention::Defer {
                reason: decision.reason,
                retry_at_ms: decision.retry_at_ms,
            };
        }
        match decision.action {
            Some(action) => SafetyIntervention::Hold {
                action,
                reason: decision.reason,
            },
            None => SafetyIntervention::Proceed,
        }
    }
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
    push_if(physical_service(input), &mut found);
    push_if(import_ceiling(input, state, now_ms), &mut found);
    push_if(full_charge(input), &mut found);
    push_if(export_guard(input), &mut found);
    push_if(stale_pack(input), &mut found);
    push_if(observation_budget(input, request), &mut found);
    push_if(orphan_hold(input), &mut found);
    push_if(cancel_ramp(input), &mut found);
    push_if(restart_grace(input, now_ms), &mut found);
    push_if(unverified_hold(state, input, request), &mut found);
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
    let mut reasons: Vec<&'static str> = Vec::new();
    for item in &ordered {
        reasons.push(item.reason);
        reasons.extend(item.extra_reasons.iter().copied());
    }
    let Some(winner) = ordered
        .iter()
        .find(|item| item.action.is_some() || item.defer_until_ms.is_some())
    else {
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
        outcome: if winner.defer_until_ms.is_some() {
            DecisionOutcome::Deferred
        } else if winner.blocked {
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
        retry_at_ms: winner.defer_until_ms,
    }
}
