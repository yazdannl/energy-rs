//! M5 override order, safety-lane exemption and physical-limit precedence.

use energy_decide::dispatch::{SafetyAction, SafetyGate, SlotRequest};
use energy_decide::safety::{EMERGENCY_CHARGE_W, RULE_ORDER, Rule, SafetyInput, SafetyLadder};

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
        (4, "grid_import_ceiling"),
        (5, "full_charge_protection"),
        (7, "stale_sample_unreachable"),
        (8, "orphan_hold"),
        (9, "cancel_ramp"),
        (10, "restart_grace"),
        (11, "controller_uncertain"),
        (12, "ordinary_backoff"),
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
    assert!(ladder.last_decision("P").is_some());
    let decision = ladder
        .last_decision("P")
        .unwrap_or_else(|| ladder.decide(&safety("P", 0.0, 20_000), 20_000));
    assert_eq!(decision.reason, "cell_voltage_guard");
    assert_eq!(decision.rule, Rule::CellLatch);
    assert!(matches!(
        intervention,
        energy_decide::dispatch::SafetyIntervention::Hold { .. }
    ));
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
        Some(SafetyAction::EmergencyCharge {
            target_w: EMERGENCY_CHARGE_W,
        })
    );
    assert!(allowed.reasons.contains(&"grid_telemetry_missing"));
}

#[test]
fn s278_import_latch_releases_only_on_settled_grid() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7900.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    let ladder = SafetyLadder::new();
    ladder.observe(input.clone());
    let breached = ladder.decide(&economic("P", 0.0, 0), 0);
    assert_eq!(breached.rule, Rule::ImportCeiling);

    input.grid_w = Some(7400.0);
    ladder.observe(input.clone());
    let transient = ladder.decide(&economic("P", 0.0, 20_000), 20_000);
    assert_eq!(transient.rule, Rule::ImportCeiling);
    assert!(transient.reasons.contains(&"grid_import_ceiling"));

    input.settled_grid_w = Some(7480.0);
    ladder.observe(input);
    let released = ladder.decide(&economic("P", 0.0, 40_000), 40_000);
    assert_eq!(released.action, None);
    assert!(!released.reasons.contains(&"grid_import_ceiling"));
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
        energy_decide::dispatch::SafetyIntervention::Hold { action, reason } => (action, reason),
        energy_decide::dispatch::SafetyIntervention::Proceed => {
            (SafetyAction::NeutralHold, "unexpected_proceed")
        }
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
    assert_eq!(RULE_ORDER.len(), 14);
    assert_eq!(RULE_ORDER[13], Rule::Ordinary);
    let split = missing
        .reasons
        .iter()
        .position(|r| *r == "solar_telemetry_missing")
        .unwrap_or(usize::MAX);
    assert_eq!(split, missing.reasons.len() - 1);
}
