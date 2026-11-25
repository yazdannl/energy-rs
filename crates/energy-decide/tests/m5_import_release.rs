//! M5 import-latch convergence, proof-only recovery and audited manual release regressions.

use energy_decide::dispatch::{SafetyAction, SlotRequest};
use energy_decide::safety::{IMPORT_CONVERGENCE_MARGIN_W, Rule, SafetyInput, SafetyLadder};

fn economic(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::economic(pack, requested_w, at_ms, "plan")
}

fn armed_ladder() -> (SafetyLadder, SafetyInput) {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7_900.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    ladder.observe(input.clone());
    (ladder, input)
}

#[test]
fn s304_remedy_converges_below_the_release_threshold() {
    let (ladder, mut input) = armed_ladder();
    let breach = ladder.decide(&economic("P", 0.0, 0), 0);
    let action = breach.action.unwrap_or(SafetyAction::NeutralHold);
    assert!(matches!(action, SafetyAction::ProtectiveDischarge { .. }));
    let target_w = action.target_w();
    let release_band = 7_500.0 - 10.0 - IMPORT_CONVERGENCE_MARGIN_W;
    assert!(target_w <= release_band - 7_900.0, "target {target_w} W");
    assert!(breach.reasons.contains(&"settled_grid_evidence_missing"));
    ladder.recover_hold("P", 1);

    // A load stable at the ceiling is still above the release threshold.
    input.grid_w = Some(7_500.0);
    input.grid_sample_at_ms = Some(10_000);
    input.last_sample_at_ms = 9_000;
    input.settled_grid_w = Some(7_500.0);
    input.settled_at_ms = Some(10_000);
    ladder.observe(input.clone());
    let stable = ladder.decide(&economic("P", 0.0, 20_000), 20_000);
    assert_eq!(stable.rule, Rule::Ordinary);
    assert!(ladder.import_protection_active("P"));
    ladder.recover_hold("P", 20_001);

    // Settled evidence inside the release band clears the guard.
    input.grid_w = Some(7_440.0);
    input.grid_sample_at_ms = Some(30_000);
    input.last_sample_at_ms = 29_000;
    input.settled_grid_w = Some(7_440.0);
    input.settled_at_ms = Some(30_000);
    ladder.observe(input);
    let released = ladder.decide(&economic("P", 0.0, 40_000), 40_000);
    assert_eq!(released.rule, Rule::Ordinary);
    assert!(!ladder.import_protection_active("P"));
}

#[test]
fn s307_unavailable_remedy_is_recorded_explicitly() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7_900.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    input.max_discharge_w = 100.0;
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 0.0, 0), 0);
    assert_eq!(decision.reason, "import_remedy_unavailable");
    assert!(decision.binding_limits.contains(&"max_discharge_w"));
    assert!(decision.reasons.contains(&"settled_grid_evidence_missing"));
    assert_eq!(decision.action, Some(SafetyAction::NeutralHold));
    assert_eq!(decision.command_w, 0.0);
    assert!(ladder.import_protection_active("P"));
}

#[test]
fn s305_hold_recovery_is_proof_only_and_reports_import_protection() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    let status = ladder.recover_hold("P", 1);
    assert!(status.hold_proof_cleared);
    assert!(status.import_protection_active);
    assert!(ladder.hold_proof("P").is_none());
    assert!(
        ladder
            .last_decision("P")
            .is_some_and(|decision| decision.steps.contains(&"import_protection_still_active"))
    );

    input.grid_w = Some(7_400.0);
    input.grid_sample_at_ms = Some(2);
    input.last_sample_at_ms = 1;
    input.settled_grid_w = Some(7_400.0);
    input.settled_at_ms = Some(1);
    ladder.observe(input);
    let _ = ladder.release_import("P", "operator", "latch isolated", 2);
    let cleared = ladder.recover_hold("P", 3);
    assert!(!cleared.import_protection_active);
}

#[test]
fn s306_manual_import_release_is_guarded_and_audited() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);

    assert_eq!(
        ladder.release_import("P", "operator", "", 1),
        Err("manual_release_audit_required")
    );
    assert_eq!(
        ladder.release_import("P", "operator", "isolated import breach", 1),
        Err("import_breach_active")
    );
    assert!(ladder.import_protection_active("P"));

    input.grid_w = Some(7_400.0);
    input.grid_sample_at_ms = Some(2);
    input.last_sample_at_ms = 1;
    input.settled_grid_w = Some(7_400.0);
    input.settled_at_ms = Some(1);
    ladder.observe(input);
    let record = ladder
        .release_import("P", "operator-7", "load isolated and verified", 2)
        .unwrap_or_default();
    assert_eq!(record.pack_id, "P");
    assert_eq!(record.actor, "operator-7");
    assert_eq!(record.reason, "load isolated and verified");
    assert_eq!(record.at_ms, 2);
    assert!(ladder.import_release_history().contains(&record));
    assert!(!ladder.import_protection_active("P"));
    assert!(
        ladder
            .last_decision("P")
            .is_some_and(|decision| decision.steps.contains(&"import_guard_manually_released"))
    );
}

#[test]
fn s308_settled_remedy_releases_the_latch_but_keeps_protecting_the_proposal() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    ladder.recover_hold("P", 1);

    input.import_armed = false;
    input.grid_w = Some(7_440.0);
    input.battery_w = Some(-800.0);
    input.physical_service_limit_w = 10_000.0;
    input.grid_sample_at_ms = Some(20_000);
    input.last_sample_at_ms = 19_000;
    input.settled_grid_w = Some(7_440.0);
    input.settled_at_ms = Some(10_000);
    ladder.observe(input.clone());

    let released = ladder.decide(&economic("P", 800.0, 20_000), 20_000);
    assert!(!ladder.import_protection_active("P"));
    assert_eq!(released.rule, Rule::ImportCeiling);
    assert_eq!(
        released.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -800.0 })
    );

    input.grid_sample_at_ms = Some(40_000);
    input.last_sample_at_ms = 39_000;
    input.settled_at_ms = Some(30_000);
    ladder.observe(input.clone());
    let after_release = ladder.decide(&economic("P", 800.0, 40_000), 40_000);
    assert!(!ladder.import_protection_active("P"));
    assert_eq!(
        after_release.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -800.0 })
    );
    assert!(
        !after_release
            .reasons
            .contains(&"settled_grid_evidence_missing")
    );
}

#[test]
fn s309_manual_release_record_captures_engine_belief() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    input.grid_w = Some(7_400.0);
    input.import_armed = false;
    input.grid_sample_at_ms = Some(2);
    input.last_sample_at_ms = 1;
    input.settled_grid_w = Some(7_400.0);
    input.settled_at_ms = Some(1);
    ladder.observe(input);
    let record = ladder
        .release_import("P", "operator-7", "load isolated and verified", 2)
        .unwrap_or_default();

    assert_eq!(record.projected_import_w, Some(7_400.0));
    assert_eq!(record.measured_grid_w, Some(7_400.0));
    assert_eq!(record.proposed_plan_w, 0.0);
    assert_eq!(record.release_threshold_w, 7_490.0);
    assert_eq!(record.import_ceiling_w, 7_500.0);
    assert_eq!(record.import_deadband_w, 10.0);
    assert_eq!(record.evidence_window_start_ms, 0);
    assert_eq!(record.evidence_window_end_ms, 2);
    assert_eq!(record.settled_grid_w, Some(7_400.0));
    assert_eq!(record.settled_at_ms, Some(1));
    assert!(!record.breach_now);
    assert!(!record.proposed_plan_breach);
    assert!(record.breach_remedied);
}

#[test]
fn s310_aged_evidence_does_not_rearm_the_latch_without_a_new_breach() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    ladder.recover_hold("P", 1);

    input.import_armed = false;
    input.grid_w = Some(7_440.0);
    input.battery_w = Some(-800.0);
    input.physical_service_limit_w = 10_000.0;
    input.grid_sample_at_ms = Some(70_000);
    input.last_sample_at_ms = 69_000;
    input.settled_grid_w = Some(7_440.0);
    input.settled_at_ms = Some(10_000);
    input.grid_sample_at_ms = Some(20_000);
    input.last_sample_at_ms = 19_000;
    ladder.observe(input.clone());
    let _ = ladder.decide(&economic("P", 800.0, 20_000), 20_000);
    ladder.recover_hold("P", 20_001);
    assert!(!ladder.import_protection_active("P"));

    input.grid_sample_at_ms = Some(70_000);
    input.last_sample_at_ms = 69_000;
    input.settled_at_ms = Some(30_000);
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 800.0, 70_000), 70_000);
    assert!(!ladder.import_protection_active("P"));
    assert_eq!(ladder.import_episode_start_ms("P"), 0);
    assert_eq!(
        decision.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -800.0 })
    );
}

#[test]
fn s311_new_measured_breach_starts_a_new_episode() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    ladder.recover_hold("P", 1);

    input.import_armed = false;
    input.grid_w = Some(7_900.0);
    input.battery_w = Some(0.0);
    input.physical_service_limit_w = 10_000.0;
    input.grid_sample_at_ms = Some(80_000);
    input.last_sample_at_ms = 79_000;
    input.settled_grid_w = Some(7_440.0);
    input.settled_at_ms = Some(30_000);
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 800.0, 80_000), 80_000);
    assert!(ladder.import_protection_active("P"));
    assert_eq!(ladder.import_episode_start_ms("P"), 80_000);
    assert_eq!(
        decision.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -460.0 })
    );
}

#[test]
fn s312_recurring_import_flag_cannot_arm_or_rearm_the_latch() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    ladder.recover_hold("P", 1);
    assert!(ladder.import_protection_active("P"));

    input.grid_w = Some(7_400.0);
    input.grid_sample_at_ms = Some(10_000);
    input.last_sample_at_ms = 9_000;
    input.settled_grid_w = Some(7_400.0);
    input.settled_at_ms = Some(10_000);
    ladder.observe(input.clone());
    let _ = ladder.decide(&economic("P", 0.0, 20_000), 20_000);
    assert!(!ladder.import_protection_active("P"));

    input.grid_sample_at_ms = Some(40_000);
    input.last_sample_at_ms = 39_000;
    ladder.observe(input);
    let _ = ladder.decide(&economic("P", 0.0, 40_000), 40_000);
    assert!(
        !ladder.import_protection_active("P"),
        "a persistent import flag must not start a new episode"
    );
}

#[test]
fn s313_manual_release_requires_settled_remediation_evidence() {
    let (ladder, mut input) = armed_ladder();
    ladder.decide(&economic("P", 0.0, 0), 0);
    ladder.recover_hold("P", 1);

    input.grid_w = Some(7_400.0);
    ladder.observe(input.clone());
    assert_eq!(
        ladder.release_import("P", "operator", "looks clear", 2),
        Err("settled_remediation_required")
    );
    assert!(ladder.import_protection_active("P"));

    input.grid_sample_at_ms = Some(2);
    input.last_sample_at_ms = 1;
    input.settled_grid_w = Some(7_400.0);
    input.settled_at_ms = Some(1);
    ladder.observe(input);
    let record = ladder
        .release_import("P", "operator", "settled evidence", 2)
        .unwrap_or_default();
    assert!(record.breach_remedied);
    assert!(!ladder.import_protection_active("P"));
}

#[test]
fn s314_proposal_inside_deadband_is_not_safe() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7_505.0);
    input.battery_w = Some(0.0);
    input.physical_service_limit_w = 9_000.0;
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 0.0, 0), 0);
    assert_eq!(decision.rule, Rule::ImportCeiling);
    assert_eq!(
        decision.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -65.0 })
    );
}

#[test]
fn s316_nonfinite_settled_evidence_cannot_release_automatically() {
    for settled_w in [f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        let (ladder, mut input) = armed_ladder();
        ladder.decide(&economic("P", 0.0, 0), 0);
        ladder.recover_hold("P", 1);

        input.import_armed = false;
        input.grid_w = Some(7_400.0);
        input.grid_sample_at_ms = Some(20_000);
        input.last_sample_at_ms = 19_000;
        input.settled_grid_w = Some(settled_w);
        input.settled_at_ms = Some(10_000);
        ladder.observe(input);
        let _ = ladder.decide(&economic("P", 0.0, 20_000), 20_000);
        assert!(
            ladder.import_protection_active("P"),
            "settled {settled_w:?} must not release"
        );
    }
}

#[test]
fn s317_nonfinite_settled_evidence_cannot_release_manually() {
    for settled_w in [f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        let (ladder, mut input) = armed_ladder();
        ladder.decide(&economic("P", 0.0, 0), 0);
        ladder.recover_hold("P", 1);

        input.import_armed = false;
        input.grid_w = Some(7_400.0);
        input.grid_sample_at_ms = Some(2);
        input.last_sample_at_ms = 1;
        input.settled_grid_w = Some(settled_w);
        input.settled_at_ms = Some(1);
        ladder.observe(input);
        let result = ladder.release_import("P", "operator", "settled evidence", 2);
        assert_eq!(
            result,
            Err("settled_remediation_required"),
            "settled {settled_w:?}"
        );
        assert!(
            ladder.import_protection_active("P"),
            "settled {settled_w:?} must not release"
        );
    }
}
