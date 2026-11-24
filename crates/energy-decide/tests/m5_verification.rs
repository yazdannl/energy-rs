//! M5 hold verification, bounded escalation and honest M4 safety reporting.

mod m4_support;

use std::sync::Arc;

use energy_decide::dispatch::{
    AllowAllAuthority, DispatchOutcome, DispatchRow, DispatchState, Dispatcher, Echo, EvidenceKind,
    FlowReport, PackCommand, SafetyAction, SafetyExecution, SafetyGate, SafetyIntervention,
    SlotRequest, safety_execution_row,
};
use energy_decide::safety::{
    HoldProof, Rule, SAFETY_ATTEMPT_DEADLINE_MS, SafetyInput, SafetyLadder, SafetyReceipt, from_row,
};
use m4_support::{Behavior, FakeIo, ImmediateClock};

struct SharedGate(Arc<SafetyLadder>);

impl SafetyGate for SharedGate {
    fn preempt(&self, request: &SlotRequest, now_ms: i64) -> SafetyIntervention {
        self.0.preempt(request, now_ms)
    }
}

fn economic(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::economic(pack, requested_w, at_ms, "plan")
}

fn open_proof(ladder: &SafetyLadder) -> HoldProof {
    ladder
        .hold_proof("P")
        .unwrap_or_else(|| HoldProof::new(0, 0, 0.0, SAFETY_ATTEMPT_DEADLINE_MS.cast_signed()))
}

fn receipt(ladder: &SafetyLadder, report: FlowReport) -> SafetyReceipt {
    let proof = open_proof(ladder);
    SafetyReceipt {
        pack_id: "P".to_owned(),
        safety_seq: proof.safety_seq,
        attempt_id: proof.attempt_id,
        report,
        emergency_resolved: None,
    }
}

fn direct_receipt(proof: &HoldProof, report: FlowReport) -> SafetyReceipt {
    SafetyReceipt {
        pack_id: "P".to_owned(),
        safety_seq: proof.safety_seq,
        attempt_id: proof.attempt_id,
        report,
        emergency_resolved: None,
    }
}

fn observe(proof: &mut HoldProof, report: FlowReport, now_ms: i64) {
    let receipt = direct_receipt(proof, report);
    proof.observe_receipt(&receipt, now_ms);
}

fn idle_report(pack: &str, at_ms: i64, measured_w: f64, message_id: u32) -> FlowReport {
    FlowReport {
        pack_id: pack.to_owned(),
        at_ms,
        measured_w,
        ac_status: 0,
        dc_status: 0,
        fault_level: 0,
        soc_pct: 50.0,
        device_min_soc_pct: 10.0,
        echo: None,
        message_id,
        device_clock_ok: true,
        device_clock_coherent: true,
    }
}

fn echoed(report: FlowReport, limit_w: f64) -> FlowReport {
    let mut report = report;
    report.echo = Some(Echo {
        limit_w,
        at_ms: report.at_ms,
    });
    report
}

#[test]
fn s266_echo_alone_never_verifies_a_neutral_hold() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    ladder.observe(input);
    let decision = ladder.decide(&economic("P", -800.0, 0), 0);
    assert!(ladder.hold_proof("P").is_some());

    ladder.observe_hold(
        &receipt(&ladder, echoed(idle_report("P", 20, 0.0, 1), 0.0)),
        20,
    );
    assert!(!open_proof(&ladder).verified);
    assert!(open_proof(&ladder).echo_seen);

    ladder.observe_hold(&receipt(&ladder, idle_report("P", 40, 450.0, 2)), 40);
    assert!(!open_proof(&ladder).verified);
    assert!(open_proof(&ladder).steps.contains(&"flow_not_neutral"));

    ladder.observe_hold(&receipt(&ladder, idle_report("P", 60, 0.0, 3)), 60);
    assert!(!open_proof(&ladder).verified);
    ladder.observe_hold(&receipt(&ladder, idle_report("P", 80, 0.0, 4)), 80);
    let proof = open_proof(&ladder);
    assert!(proof.verified);
    assert_eq!(proof.neutral_reports, vec![60, 80]);
    assert!(proof.steps.contains(&"hold_verified_by_measured_flow"));
    assert!(decision.steps.contains(&"rules_evaluated"));
}

#[test]
fn s096_orphan_hold_is_verified_by_measured_neutrality() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.ownership_lost = true;
    input.standing_command_w = -800.0;
    ladder.observe(input);
    let decision = ladder.decide(&economic("P", 0.0, 0), 0);
    assert_eq!(decision.reason, "orphan_hold");
    assert_eq!(decision.action, Some(SafetyAction::NeutralHold));
    ladder.observe_hold(
        &receipt(&ladder, echoed(idle_report("P", 20, 0.0, 1), 0.0)),
        20,
    );
    ladder.observe_hold(&receipt(&ladder, idle_report("P", 40, 0.0, 2)), 40);
    ladder.observe_hold(&receipt(&ladder, idle_report("P", 60, 0.0, 3)), 60);
    assert!(open_proof(&ladder).verified);
}

#[test]
fn s097_cancel_ramp_holds_before_a_changed_target() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cancel_ramp = true;
    input.standing_command_w = -800.0;
    ladder.observe(input);
    let decision = ladder.decide(&economic("P", 200.0, 0), 0);
    assert_eq!(decision.reason, "cancel_ramp");
    assert_eq!(decision.action, Some(SafetyAction::NeutralHold));
    assert_eq!(decision.command_w, 0.0);
}

#[test]
fn s098_restart_grace_never_delays_a_cell_hold() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.restart_at_ms = Some(0);
    input.cell_latch = true;
    input.standing_command_w = -800.0;
    ladder.observe(input.clone());
    let safety = ladder.decide(&economic("P", -800.0, 1_000), 1_000);
    assert_eq!(safety.reason, "cell_voltage_guard");
    assert_eq!(safety.rule, Rule::CellLatch);

    ladder.recover_hold("P", 1_001);
    input.cell_latch = false;
    ladder.observe(input.clone());
    let graced = ladder.decide(&economic("P", 200.0, 2_000), 2_000);
    assert_eq!(graced.reason, "restart_grace");
    ladder.recover_hold("P", 2_001);
    input.restart_at_ms = Some(-120_001);
    ladder.observe(input);
    let expired = ladder.decide(&economic("P", 200.0, 2_000), 2_000);
    assert!(!expired.reasons.contains(&"restart_grace"));
}

#[test]
fn s099_no_command_standing_has_an_explicit_no_action_reason() {
    let ladder = SafetyLadder::new();
    ladder.observe(SafetyInput::new("P", 0));
    let decision = ladder.decide(&economic("P", 0.0, 0), 0);
    assert_eq!(decision.action, None);
    assert!(decision.reasons.contains(&"no_command_needed"));
    assert!(decision.reasons.contains(&"no_safety_hazard"));
}

#[test]
fn s137_receipt_must_match_both_safety_seq_and_attempt_id() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    ladder.observe(input);
    ladder.decide(&economic("P", -800.0, 0), 0);
    ladder.fail_hold("P", 0, "ack_timeout");

    let proof = open_proof(&ladder);
    assert_eq!(proof.attempt_id, 2);
    let mut stale = receipt(&ladder, echoed(idle_report("P", 2_100, 0.0, 1), 0.0));
    stale.attempt_id = 1;
    let current = receipt(&ladder, echoed(idle_report("P", 2_100, 0.0, 2), 0.0));
    ladder.observe_hold(&stale, 2_100);
    assert!(open_proof(&ladder).steps.contains(&"late_uncorrelated"));
    assert_eq!(open_proof(&ladder).neutral_reports, Vec::<i64>::new());

    ladder.observe_hold(&current, 2_100);
    let mut later = current;
    later.report.at_ms = 2_200;
    later.report.message_id = 3;
    ladder.observe_hold(&later, 2_200);
    assert!(open_proof(&ladder).echo_seen);
    assert_eq!(open_proof(&ladder).neutral_reports, vec![2_200]);
}

#[test]
fn s267_safety_retries_are_bounded_and_escalate_visibly() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    input.backoff_until_ms = 300_000;
    ladder.observe(input);
    ladder.decide(&economic("P", -800.0, 0), 0);

    ladder.fail_hold("P", 0, "ack_timeout");
    let mut at_ms = open_proof(&ladder).escalation.retry_at_ms.unwrap_or(-1);
    assert_eq!(at_ms, 2_000);
    ladder.fail_hold("P", at_ms, "ramp_end_unverified");
    at_ms = open_proof(&ladder).escalation.retry_at_ms.unwrap_or(-1);
    assert_eq!(at_ms, 8_000);
    ladder.fail_hold("P", at_ms, "receipt_missing");

    let proof = open_proof(&ladder);
    assert!(proof.escalation.exhausted());
    assert_eq!(proof.escalation.retry_at_ms, None);
    assert_eq!(proof.escalation.path, "ownership_retention");
    for step in [
        "ack_timeout",
        "ramp_end_unverified",
        "receipt_missing",
        "safety_write_unverified",
        "ownership_retention",
    ] {
        assert!(
            proof.steps.contains(&step),
            "missing escalation step {step}"
        );
    }
    let later = ladder.decide(&economic("P", -800.0, 10_000), 10_000);
    assert_eq!(
        later.outcome,
        energy_decide::safety::DecisionOutcome::Failed
    );
    assert!(later.reasons.contains(&"ordinary_backoff"));
    assert_eq!(open_proof(&ladder).safety_seq, proof.safety_seq);
}

#[test]
fn s283_neutral_evidence_must_be_distinct_ordered_and_unbroken() {
    let mut proof = HoldProof::new(1, 0, 0.0, SAFETY_ATTEMPT_DEADLINE_MS.cast_signed());
    observe(&mut proof, echoed(idle_report("P", 20, 0.0, 1), 0.0), 20);
    observe(&mut proof, idle_report("P", 40, 0.0, 2), 40);
    observe(&mut proof, idle_report("P", 40, 0.0, 3), 40);
    observe(&mut proof, idle_report("P", 30, 0.0, 4), 40);
    assert_eq!(proof.neutral_reports, vec![40]);
    assert!(proof.steps.contains(&"duplicate_neutral_report"));
    assert!(proof.steps.contains(&"non_chronological_report"));

    observe(&mut proof, idle_report("P", 50, 450.0, 5), 50);
    assert_eq!(proof.neutral_reports, Vec::<i64>::new());
    assert!(!proof.verified);
    assert!(proof.steps.contains(&"neutral_run_invalidated"));

    observe(&mut proof, idle_report("P", 60, 0.0, 6), 60);
    observe(&mut proof, idle_report("P", 80, 0.0, 7), 80);
    assert!(proof.verified);

    observe(&mut proof, idle_report("P", 100, -500.0, 8), 100);
    assert!(!proof.verified);
    assert!(proof.steps.contains(&"hold_reprove_required"));
    observe(&mut proof, echoed(idle_report("P", 120, 0.0, 9), 0.0), 120);
    observe(&mut proof, idle_report("P", 140, 0.0, 10), 140);
    observe(&mut proof, idle_report("P", 160, 0.0, 11), 160);
    assert!(proof.verified);
}

#[test]
fn s284_a_retry_never_inherits_the_previous_attempts_evidence() {
    let mut proof = HoldProof::new(1, 0, 0.0, SAFETY_ATTEMPT_DEADLINE_MS.cast_signed());
    observe(&mut proof, echoed(idle_report("P", 10, 0.0, 1), 0.0), 10);
    observe(&mut proof, idle_report("P", 20, 0.0, 2), 20);
    proof.fail(30, "ack_timeout");
    assert_eq!(proof.attempt_id, 2);
    assert!(!proof.echo_seen);
    assert_eq!(proof.neutral_reports, Vec::<i64>::new());

    let mut old_echo = direct_receipt(&proof, echoed(idle_report("P", 40, 0.0, 3), 0.0));
    old_echo.attempt_id = 1;
    let mut old_neutral = direct_receipt(&proof, idle_report("P", 50, 0.0, 4));
    old_neutral.attempt_id = 1;
    proof.observe_receipt(&old_echo, 40);
    proof.observe_receipt(&old_neutral, 50);

    proof.advance_due(2_000);
    observe(&mut proof, echoed(idle_report("P", 60, 0.0, 5), 0.0), 60);
    observe(&mut proof, idle_report("P", 70, 0.0, 6), 70);
    assert!(
        !proof.verified,
        "one new neutral must not reuse attempt 1 evidence"
    );
    observe(&mut proof, idle_report("P", 90, 0.0, 7), 90);
    assert!(proof.verified);
}

#[test]
fn s285_elapsed_ticks_advance_bounded_escalation_and_retain_terminal_state() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    ladder.observe(input);
    let request = economic("P", -800.0, 0);
    let deadline = SAFETY_ATTEMPT_DEADLINE_MS.cast_signed();

    ladder.decide(&request, 0);
    let first_seq = open_proof(&ladder).safety_seq;
    ladder.decide(&request, deadline);
    let proof = open_proof(&ladder);
    assert_eq!(proof.escalation.failures, 1);
    assert_eq!(proof.escalation.retry_at_ms, Some(deadline + 2_000));

    let first_retry = open_proof(&ladder).escalation.retry_at_ms.unwrap_or(-1);
    ladder.decide(&request, first_retry);
    assert_eq!(open_proof(&ladder).attempt_id, 2);
    let second_deadline = open_proof(&ladder).deadline_ms();
    ladder.decide(&request, second_deadline);
    assert_eq!(open_proof(&ladder).escalation.failures, 2);
    let second_retry = open_proof(&ladder).escalation.retry_at_ms.unwrap_or(-1);
    ladder.decide(&request, second_retry);
    assert_eq!(open_proof(&ladder).attempt_id, 3);

    let exhausted_at = open_proof(&ladder).deadline_ms();
    let exhausted = ladder.decide(&request, exhausted_at);
    assert_eq!(
        exhausted.outcome,
        energy_decide::safety::DecisionOutcome::Failed
    );
    assert!(open_proof(&ladder).escalation.exhausted());
    let beyond = ladder.decide(&request, exhausted_at + 60_000);
    assert_eq!(
        beyond.outcome,
        energy_decide::safety::DecisionOutcome::Failed
    );
    assert_eq!(open_proof(&ladder).safety_seq, first_seq);
    assert_eq!(open_proof(&ladder).escalation.failures, 3);
    ladder.recover_hold("P", exhausted_at + 60_001);
    assert!(ladder.hold_proof("P").is_none());
}

#[test]
fn s273_dispatcher_obeys_and_records_the_real_safety_gate() {
    let ladder = Arc::new(SafetyLadder::new());
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    ladder.observe(input);
    let dut = Dispatcher::new(
        FakeIo::new(&[("P", Behavior::Accept { echo: true })]),
        SharedGate(Arc::clone(&ladder)),
        AllowAllAuthority,
        ImmediateClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0, 0)], 0);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].safety_traffic);
    assert_eq!(rows[0].requested_w, 0.0);
    assert_eq!(rows[0].economic_preempted_by, Some("cell_voltage_guard"));
    assert!(rows[0].safety_execution.is_some());
    let decision = ladder
        .last_decision("P")
        .unwrap_or_else(|| ladder.decide(&economic("P", -800.0, 0), 0));
    assert_eq!(decision.reason, "cell_voltage_guard");
}

#[test]
fn s273_safety_execution_is_never_hidden_by_the_gate_decision() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    ladder.observe(input);
    let decision = ladder.decide(&economic("P", -800.0, 0), 0);
    let command = PackCommand {
        pack_id: "P".to_owned(),
        requested_w: 0.0,
        issued_at_ms: 0,
        generation: 1,
        fence_id: None,
        safety_traffic: true,
    };
    let row = DispatchRow::base(&command, 20_000)
        .outcome(DispatchOutcome::Rejected, EvidenceKind::PortDecision)
        .reason("device_rejected");
    let row = safety_execution_row::row(row, Some(decision.reason), false);

    let report = from_row(&row, &decision);
    assert_eq!(report.execution, Some(SafetyExecution::Rejected));
    assert_ne!(report.execution, Some(SafetyExecution::Accepted));
    assert!(report.steps.contains(&"cell_voltage_guard"));
    assert!(report.steps.contains(&"safety_execution_recorded"));
    assert_eq!(row.economic_preempted_by, Some("cell_voltage_guard"));
}
