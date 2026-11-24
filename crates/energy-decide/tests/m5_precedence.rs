//! M5 override order, projected-import guard, stale fallbacks and safety-lane exemption.

use energy_decide::dispatch::{SafetyAction, SafetyGate, SafetyIntervention, SlotRequest};
use energy_decide::safety::{
    COMMAND_SAMPLE_MAX_MS, EMERGENCY_DURATION_MS, RULE_ORDER, Rule, SafetyInput, SafetyLadder,
};

fn economic(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::economic(pack, requested_w, at_ms, "plan")
}

fn safety(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::safety(pack, requested_w, at_ms, "cell_latch")
}

#[test]
fn s260_safety_action_is_not_blocked_by_the_failure_it_protects_against() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7900.0);
    input.battery_w = Some(-500.0);
    input.import_armed = true;
    input.max_discharge_w = 1200.0;
    input.backoff_until_ms = 300_000;
    input.uncertain = true;
    input.plan_w = -800.0;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());

    let first = ladder.decide(&economic("P", -800.0, 0), 0);
    assert_eq!(first.rule, Rule::ImportCeiling);
    assert_eq!(
        first.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -900.0 })
    );
    assert!(first.reasons.contains(&"grid_import_ceiling"));
    assert!(first.reasons.contains(&"controller_uncertain"));
    assert!(first.reasons.contains(&"ordinary_backoff"));

    input.grid_w = None;
    ladder.observe(input);
    let second = ladder.decide(&economic("P", -800.0, 20_000), 20_000);
    assert_eq!(second.rule, Rule::ImportCeiling);
    assert_eq!(
        second.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -900.0 })
    );
    assert!(second.reasons.contains(&"grid_telemetry_missing"));
    assert!(second.reasons.contains(&"ordinary_backoff"));
}

#[test]
fn s265_one_ordered_rule_list_declares_the_winner_and_every_reason() {
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    input.cell_force_charge = true;
    input.cell_min_v = 2.99;
    input.grid_w = Some(7900.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    input.targets.hard_floor_kwh = 5.0;
    input.soc_kwh = 5.0;
    input.soc_pct = 100.0;
    input.targets.ceiling_kwh = 4.9;
    input.export_ceiling_w = 0.0;
    input.last_sample_at_ms = -301_000;
    input.ownership_lost = true;
    input.cancel_ramp = true;
    input.standing_command_w = -800.0;
    input.restart_at_ms = Some(0);
    input.uncertain = true;
    input.backoff_until_ms = 300_000;
    let ladder = SafetyLadder::new();
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 800.0, 0), 0);
    assert_eq!(decision.rule, Rule::CellLatch);
    assert_eq!(decision.reason, "cell_voltage_guard");
    assert_eq!(
        decision.action,
        Some(SafetyAction::NeutralHold),
        "cell latch must win over every lower rule"
    );
    let expected = [
        (1, "cell_voltage_guard"),
        (2, "emergency_charge_impossible"),
        (3, "soc_floor"),
        (4, "import_remedy_blocked"),
        (5, "full_charge_protection"),
        (7, "stale_sample_unreachable"),
        (8, "stale_command_sample"),
        (9, "orphan_hold"),
        (10, "cancel_ramp"),
        (11, "restart_grace"),
        (13, "controller_uncertain"),
        (14, "ordinary_backoff"),
    ];
    for (index, (position, reason)) in expected.iter().enumerate() {
        assert_eq!(decision.reasons[index], *reason);
        assert_eq!(RULE_ORDER[position - 1].position(), *position);
    }
    for rule in RULE_ORDER {
        assert_ne!(rule.reason(), "");
        assert_ne!(rule.command(), "");
        assert_ne!(rule.verification(), "");
        assert_ne!(rule.escalation(), "");
    }
}

#[test]
fn s268_uncertain_write_blocks_ordinary_traffic_but_not_safety_traffic() {
    let mut input = SafetyInput::new("P", 0);
    input.uncertain = true;
    input.backoff_until_ms = 300_000;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    let ordinary = ladder.decide(&economic("P", -800.0, 0), 0);
    assert_eq!(ordinary.reason, "controller_uncertain");
    assert_eq!(ordinary.action, Some(SafetyAction::NeutralHold));

    input.cell_latch = true;
    ladder.observe(input);
    let intervention = ladder.preempt(&safety("P", 0.0, 20_000), 20_000);
    let decision = ladder
        .last_decision("P")
        .unwrap_or_else(|| ladder.decide(&safety("P", 0.0, 20_000), 20_000));
    assert_eq!(decision.reason, "cell_voltage_guard");
    assert_eq!(decision.rule, Rule::CellLatch);
    assert!(matches!(intervention, SafetyIntervention::Hold { .. }));
    assert!(!decision.reasons.contains(&"ordinary_backoff"));
}

#[test]
fn s269_soft_guideline_below_floor_keeps_raw_reason() {
    let mut input = SafetyInput::new("P", 0);
    input.targets.soft_guideline_kwh = 0.78575;
    input.targets.hard_floor_kwh = 1.178_625;
    input.targets.ceiling_kwh = 7.8575;
    input.plan_w = 1000.0;
    let ladder = SafetyLadder::new();
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 1000.0, 0), 0);
    assert_eq!(decision.effective_target_kwh, 1.178_625);
    assert_eq!(decision.raw_guideline_kwh, 0.78575);
    assert_eq!(decision.action, None);
    assert!(decision.reasons.contains(&"no_safety_hazard"));
}

#[test]
fn s271_import_remedy_respects_discharge_limit_and_floor() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7900.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    input.max_discharge_w = 0.0;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    let blocked = ladder.decide(&economic("P", -800.0, 0), 0);
    assert_eq!(blocked.reason, "import_remedy_blocked");
    assert_eq!(blocked.action, Some(SafetyAction::NeutralHold));
    assert!(blocked.binding_limits.contains(&"max_discharge_w"));

    input.max_discharge_w = 1000.0;
    input.soc_kwh = input.targets.hard_floor_kwh;
    ladder.observe(input);
    let floored = ladder.decide(&economic("P", -800.0, 20_000), 20_000);
    assert_eq!(floored.rule, Rule::SocFloor);
    assert_eq!(floored.reasons[0], "soc_floor");
    assert_eq!(floored.reasons[1], "grid_import_ceiling");
    assert_eq!(floored.action, Some(SafetyAction::NeutralHold));
}

#[test]
fn s272_emergency_charge_checks_pack_limit_without_meter() {
    let mut input = SafetyInput::new("P", 0);
    input.cell_min_v = 2.99;
    input.cell_force_charge = true;
    input.max_charge_w = 0.0;
    input.grid_w = None;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    let impossible = ladder.decide(&economic("P", -800.0, 0), 0);
    assert_eq!(impossible.rule, Rule::CellEmergency);
    assert_eq!(impossible.reason, "emergency_charge_impossible");
    assert!(impossible.binding_limits.contains(&"max_charge_w"));
    assert_eq!(impossible.action, Some(SafetyAction::NeutralHold));

    input.max_charge_w = 800.0;
    ladder.observe(input);
    let allowed = ladder.decide(&economic("P", -800.0, 20_000), 20_000);
    assert_eq!(
        allowed.action,
        Some(SafetyAction::EmergencyCharge { target_w: 800.0 })
    );
    assert!(allowed.reasons.contains(&"grid_telemetry_missing"));
}

#[test]
fn s274_protective_discharge_carries_signed_w_through_the_m4_port() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7800.0);
    input.battery_w = Some(-800.0);
    input.import_armed = true;
    input.max_discharge_w = 1200.0;
    let ladder = SafetyLadder::new();
    ladder.observe(input);
    let intervention = ladder.preempt(&economic("P", -800.0, 0), 0);
    let (action, reason) = match intervention {
        SafetyIntervention::Hold { action, reason } => (action, reason),
        SafetyIntervention::Defer { .. } => (SafetyAction::NeutralHold, "unexpected_defer"),
        SafetyIntervention::Proceed => (SafetyAction::NeutralHold, "unexpected_proceed"),
    };
    assert_eq!(reason, "grid_import_ceiling");
    assert_eq!(
        action,
        SafetyAction::ProtectiveDischarge { target_w: -1100.0 }
    );
    assert_eq!(action.target_w(), -1100.0);
}

#[test]
fn s275_solar_advice_has_no_precedence_slot_and_never_changes_the_command() {
    let mut input = SafetyInput::new("P", 0);
    input.plan_w = 1000.0;
    input.load_w = Some(500.0);
    input.forecast_future_pv_kwh = 5.0;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    let clear = ladder.decide(&economic("P", 1000.0, 0), 0);
    assert_eq!(clear.solar.reason, "no_measured_pv_surplus");
    assert_eq!(clear.action, None);

    input.pv_w = None;
    ladder.observe(input);
    let missing = ladder.decide(&economic("P", 1000.0, 20_000), 20_000);
    assert_eq!(missing.solar.reason, "solar_telemetry_missing");
    assert_eq!(missing.action, None);
    assert_eq!(missing.command_w, 0.0);
    assert_eq!(missing.rule, Rule::Ordinary);
    assert_eq!(RULE_ORDER.len(), 16);
    assert_eq!(RULE_ORDER[15], Rule::Ordinary);
    let split = missing
        .reasons
        .iter()
        .position(|reason| *reason == "solar_telemetry_missing")
        .unwrap_or(usize::MAX);
    assert_eq!(split, missing.reasons.len() - 1);
}

#[test]
fn s280_projected_import_includes_requested_and_coordinated_commands() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7400.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    input.max_discharge_w = 2500.0;
    input.coordinated_pack_w = vec![800.0, 800.0, 800.0];
    let ladder = SafetyLadder::new();
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 800.0, 0), 0);
    assert_eq!(
        decision.rule,
        Rule::ImportCeiling,
        "7400 W demand plus four 800 W charges projects 10.6 kW"
    );
    assert_eq!(
        decision.action,
        Some(SafetyAction::ProtectiveDischarge { target_w: -2300.0 })
    );
    assert!(decision.reasons.contains(&"grid_import_ceiling"));
}

#[test]
fn s281_import_guard_releases_only_on_fresh_post_breach_settled_evidence() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7900.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    ladder.decide(&economic("P", 0.0, 0), 0);

    input.grid_w = Some(7900.0);
    input.settled_grid_w = Some(7400.0);
    input.settled_at_ms = Some(-1_000);
    ladder.observe(input.clone());
    let contradictory = ladder.decide(&economic("P", 0.0, 20_000), 20_000);
    assert_eq!(contradictory.rule, Rule::ImportCeiling);
    assert!(contradictory.reasons.contains(&"grid_import_ceiling"));

    input.grid_w = Some(7400.0);
    input.settled_at_ms = Some(21_000);
    ladder.observe(input.clone());
    let retained = ladder.decide(&economic("P", 0.0, 60_000), 60_000);
    assert_eq!(retained.rule, Rule::ImportCeiling);
    assert_eq!(retained.rule, Rule::ImportCeiling);

    input.settled_at_ms = Some(50_000);
    ladder.observe(input);
    let released = ladder.decide(&economic("P", 0.0, 60_000), 60_000);
    assert_eq!(released.reason, "hold_unverified");
    ladder.recover_hold("P", 60_000);
    let released = ladder.decide(&economic("P", 0.0, 60_000), 60_000);
    assert_eq!(released.action, None);
    assert!(!released.reasons.contains(&"grid_import_ceiling"));
}

#[test]
fn s282_unresolved_hold_blocks_economic_traffic_until_neutrality_is_proven() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_latch = true;
    ladder.observe(input.clone());
    ladder.decide(&economic("P", -800.0, 0), 0);

    input.cell_latch = false;
    ladder.observe(input);
    let blocked = ladder.decide(&economic("P", 800.0, 20_000), 20_000);
    assert_eq!(blocked.reason, "hold_unverified");
    assert_eq!(blocked.action, Some(SafetyAction::NeutralHold));
    assert_eq!(blocked.rule, Rule::UnverifiedHold);

    ladder.recover_hold("P", 40_000);
    let cleared = ladder.decide(&economic("P", 800.0, 40_000), 40_000);
    assert_eq!(cleared.action, None);
    assert_eq!(cleared.rule, Rule::Ordinary);
}

#[test]
fn s286_ordinary_backoff_is_a_deferral_with_a_retry_instant() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.backoff_until_ms = 60_000;
    ladder.observe(input.clone());

    let decision = ladder.decide(&economic("P", 800.0, 0), 0);
    assert_eq!(decision.rule, Rule::FaultBackoff);
    assert_eq!(decision.reason, "ordinary_backoff");
    assert_eq!(decision.retry_at_ms, Some(60_000));
    assert!(decision.action.is_none());
    assert!(matches!(
        ladder.preempt(&economic("P", 800.0, 0), 0),
        SafetyIntervention::Defer {
            reason: "ordinary_backoff",
            retry_at_ms: Some(60_000),
        }
    ));
}

#[test]
fn s287_stale_samples_block_every_nonzero_action_with_a_verified_hold() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.cell_force_charge = true;
    input.cell_min_v = 2.99;
    input.cell_observed_at_ms = Some(-COMMAND_SAMPLE_MAX_MS - 1);
    ladder.observe(input.clone());
    let emergency = ladder.decide(&economic("P", -800.0, 0), 0);
    assert_eq!(emergency.rule, Rule::CellEmergency);
    assert_eq!(emergency.reason, "emergency_charge_impossible");
    assert_eq!(emergency.action, Some(SafetyAction::NeutralHold));
    assert!(emergency.binding_limits.contains(&"cell_sample_stale"));
    ladder.recover_hold("P", 1);

    let mut imported = SafetyInput::new("P", 0);
    imported.grid_w = Some(7900.0);
    imported.battery_w = Some(0.0);
    imported.import_armed = true;
    imported.max_discharge_w = 1200.0;
    imported.last_sample_at_ms = -COMMAND_SAMPLE_MAX_MS - 1;
    ladder.observe(imported);
    let remedy = ladder.decide(&economic("P", 800.0, 2), 2);
    assert_eq!(remedy.rule, Rule::ImportCeiling);
    assert_eq!(remedy.action, Some(SafetyAction::NeutralHold));
    assert!(remedy.binding_limits.contains(&"sample_stale"));
    ladder.recover_hold("P", 3);

    let mut ordinary = SafetyInput::new("P", 0);
    ordinary.last_sample_at_ms = -COMMAND_SAMPLE_MAX_MS - 1;
    let ordinary_ladder = SafetyLadder::new();
    ordinary_ladder.observe(ordinary);
    let charge = ordinary_ladder.decide(&economic("P", 800.0, 4), 4);
    assert_eq!(charge.reason, "stale_command_sample");
    assert_eq!(charge.action, Some(SafetyAction::NeutralHold));
}

#[test]
fn s288_emergency_charge_is_minimum_bounded_and_duration_limited() {
    let mut input = SafetyInput::new("P", 0);
    input.cell_force_charge = true;
    input.cell_min_v = 3.16;
    input.max_charge_w = 1200.0;
    input.grid_w = None;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    let small_deficit = ladder.decide(&economic("P", 5_000.0, 0), 0);
    assert!(matches!(
        small_deficit.action,
        Some(SafetyAction::EmergencyCharge { .. })
    ));
    let target_w = match small_deficit.action {
        Some(SafetyAction::EmergencyCharge { target_w }) => target_w,
        _ => 0.0,
    };
    assert!(target_w > 0.0 && target_w < 800.0, "got {target_w} W");
    assert!(
        small_deficit
            .reasons
            .contains(&"cell_voltage_emergency_charge")
    );
    ladder.recover_hold("P", 1);

    input.cell_min_v = 2.99;
    ladder.observe(input.clone());
    let large_deficit = ladder.decide(&economic("P", 5_000.0, 2), 2);
    assert_eq!(
        large_deficit.action,
        Some(SafetyAction::EmergencyCharge { target_w: 800.0 })
    );
    assert_eq!(
        ladder
            .hold_proof("P")
            .map_or(0, |proof| proof.attempt_duration_ms),
        EMERGENCY_DURATION_MS
    );

    input.grid_w = Some(7900.0);
    input.battery_w = Some(0.0);
    ladder.observe(input.clone());
    let fuse = ladder.decide(&economic("P", 5_000.0, 3), 3);
    assert_eq!(fuse.reason, "emergency_charge_impossible");
    assert!(fuse.binding_limits.contains(&"physical_service_limit"));

    input.physical_service_limit_w = 9_000.0;
    ladder.observe(input);
    let margin = ladder.decide(&economic("P", 5_000.0, 4), 4);
    assert_eq!(
        margin.action,
        Some(SafetyAction::EmergencyCharge { target_w: 800.0 })
    );
    assert!(margin.reasons.contains(&"import_margin_breached"));
}
