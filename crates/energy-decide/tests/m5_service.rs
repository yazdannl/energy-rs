//! M5 round-3 service-fuse, fleet-projection and observation-budget regressions.

use energy_decide::dispatch::{SafetyAction, SlotRequest};
use energy_decide::safety::{
    COMMAND_SAMPLE_MAX_MS, FUTURE_SKEW_MS, HoldProof, Rule, SAFETY_ATTEMPT_DEADLINE_MS,
    SafetyInput, SafetyLadder, SafetyReceipt, UNOBSERVED_BUDGET_MS,
};

fn economic(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::economic(pack, requested_w, at_ms, "plan")
}

fn direct_proof() -> HoldProof {
    HoldProof::new(1, 0, 0.0, SAFETY_ATTEMPT_DEADLINE_MS.cast_signed())
}

fn direct_receipt(proof: &HoldProof, at_ms: i64) -> SafetyReceipt {
    SafetyReceipt {
        pack_id: "P".to_owned(),
        safety_seq: proof.safety_seq,
        attempt_id: proof.attempt_id,
        report: energy_decide::dispatch::FlowReport {
            pack_id: "P".to_owned(),
            at_ms,
            measured_w: 0.0,
            ac_status: 0,
            dc_status: 0,
            fault_level: 0,
            soc_pct: 50.0,
            device_min_soc_pct: 10.0,
            echo: Some(energy_decide::dispatch::Echo {
                limit_w: 0.0,
                at_ms,
            }),
            message_id: 1,
            device_clock_ok: true,
            device_clock_coherent: true,
        },
        emergency_resolved: None,
    }
}

#[test]
fn s290_service_fuse_is_unconditional_and_fails_closed() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7900.0);
    input.battery_w = Some(0.0);
    input.grid_sample_at_ms = Some(0);
    ladder.observe(input.clone());

    let breach = ladder.decide(&economic("P", 1000.0, 0), 0);
    assert_eq!(breach.rule, Rule::PhysicalServiceLimit);
    assert_eq!(
        breach.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -200.0 })
    );
    ladder.recover_hold("P", 1);

    input.grid_sample_at_ms = None;
    ladder.observe(input.clone());
    let unknown = ladder.decide(&economic("P", 1000.0, 2), 2);
    assert_eq!(unknown.reason, "service_headroom_unavailable");
    assert_eq!(unknown.action, Some(SafetyAction::NeutralHold));
    assert!(
        unknown
            .binding_limits
            .contains(&"grid_sample_missing_or_stale")
    );
    ladder.recover_hold("P", 3);

    input.grid_sample_at_ms = Some(70_000);
    ladder.observe(input.clone());
    let future = ladder.decide(&economic("P", 1000.0, 4), 4);
    assert_eq!(future.reason, "service_headroom_unavailable");
    ladder.recover_hold("P", 5);

    input.grid_sample_at_ms = Some(6);
    input.physical_service_limit_w = 9_000.0;
    input.import_ceiling_w = 9_000.0;
    let configured_ladder = SafetyLadder::new();
    configured_ladder.observe(input);
    let configured = configured_ladder.decide(&economic("P", 1000.0, 6), 6);
    assert_eq!(configured.rule, Rule::Ordinary);
    assert_eq!(configured.action, None);
}

#[test]
fn s291_intermittent_fresh_samples_do_not_reauthorise_a_charge() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.last_sample_at_ms = 0;
    ladder.observe(input.clone());
    let authorised = ladder.decide(&economic("P", 1000.0, 0), 0);
    assert_eq!(authorised.rule, Rule::Ordinary);

    ladder.observe(input.clone());
    let drained = ladder.decide(&economic("P", 1000.0, 30_000), 30_000);
    assert_eq!(drained.reason, "observation_budget_exhausted");
    assert_eq!(drained.action, Some(SafetyAction::NeutralHold));

    input.last_sample_at_ms = 31_000;
    input.grid_sample_at_ms = Some(31_000);
    ladder.observe(input.clone());
    let one_fresh = ladder.decide(&economic("P", 1000.0, 31_000), 31_000);
    assert_eq!(one_fresh.reason, "observation_budget_exhausted");
    ladder.recover_hold("P", 31_001);

    input.last_sample_at_ms = 31_000;
    input.grid_sample_at_ms = Some(61_000);
    ladder.observe(input);
    let stale_again = ladder.decide(&economic("P", 1000.0, 61_000), 61_000);
    assert_eq!(stale_again.reason, "observation_budget_exhausted");
    assert_eq!(stale_again.action, Some(SafetyAction::NeutralHold));
    assert_eq!(UNOBSERVED_BUDGET_MS, COMMAND_SAMPLE_MAX_MS - 20_000);
}

#[test]
fn s292_projection_uses_fleet_total_battery_and_complete_targets() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(6100.0);
    input.battery_w = Some(-1600.0);
    input.coordinated_pack_w = vec![800.0];
    input.standing_command_w = -800.0;
    ladder.observe(input.clone());

    let mixed = ladder.decide(&economic("P", 800.0, 0), 0);
    assert_eq!(mixed.rule, Rule::PhysicalServiceLimit);
    assert_eq!(
        mixed.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -800.0 })
    );

    let mut transitioned = input;
    transitioned.standing_command_w = 800.0;
    ladder.recover_hold("P", 1);
    ladder.observe(transitioned);
    let changed = ladder.decide(&economic("P", 800.0, 2), 2);
    assert_eq!(changed.action, mixed.action);
}

#[test]
fn s293_nonzero_to_neutral_switch_creates_a_neutral_proof() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_force_charge = true;
    input.cell_min_v = 2.99;
    ladder.observe(input.clone());
    let charge = ladder.decide(&economic("P", -800.0, 0), 0);
    assert_eq!(
        charge.action,
        Some(SafetyAction::EmergencyCharge { target_w: 800.0 })
    );
    let emergency_seq = ladder.hold_proof("P").map_or(0, |proof| proof.safety_seq);
    ladder.fail_hold("P", 0, "ack_timeout");

    input.cell_force_charge = false;
    input.cell_latch = true;
    ladder.observe(input);
    let hold = ladder.decide(&economic("P", -800.0, 2), 2);
    assert_eq!(hold.action, Some(SafetyAction::NeutralHold));
    let proof = ladder.hold_proof("P").unwrap_or_else(direct_proof);
    assert_eq!(proof.target_w, 0.0);
    assert_ne!(proof.safety_seq, emergency_seq);
    assert!(proof.prior_escalation.contains(&"ack_timeout"));
    assert!(proof.blocks_ordinary());
}

#[test]
fn s294_inactive_expired_and_future_receipts_cannot_verify() {
    let mut expired = direct_proof();
    expired.advance_due(SAFETY_ATTEMPT_DEADLINE_MS.cast_signed());
    let receipt = direct_receipt(&expired, SAFETY_ATTEMPT_DEADLINE_MS.cast_signed() + 100);
    expired.observe_receipt(&receipt, SAFETY_ATTEMPT_DEADLINE_MS.cast_signed() + 100);
    assert!(expired.steps.contains(&"attempt_not_active"));
    assert!(!expired.verified);

    let mut terminal = direct_proof();
    terminal.fail(0, "first");
    let retry = terminal.escalation.retry_at_ms.unwrap_or(0);
    terminal.fail(retry, "second");
    let retry = terminal.escalation.retry_at_ms.unwrap_or(0);
    terminal.fail(retry, "third");
    let receipt = direct_receipt(&terminal, retry + 100);
    terminal.observe_receipt(&receipt, retry + 100);
    assert!(terminal.steps.contains(&"attempt_not_active"));
    assert!(!terminal.verified);

    let mut future = direct_proof();
    let now = 20;
    let receipt = direct_receipt(&future, now + FUTURE_SKEW_MS + 1);
    future.observe_receipt(&receipt, now);
    assert!(future.steps.contains(&"future_dated_report"));
    assert!(!future.verified);
}
