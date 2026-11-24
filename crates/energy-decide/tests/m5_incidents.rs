//! Live 2026-11-22..24 safety incidents replayed against the M5 decision core.

use std::time::{Duration, Instant};

use energy_decide::dispatch::{SafetyGate, SlotRequest};
use energy_decide::safety::{HoldProof, SafetyInput, SafetyLadder};

fn open_proof(ladder: &SafetyLadder, pack: &str) -> HoldProof {
    ladder
        .hold_proof(pack)
        .unwrap_or_else(|| HoldProof::new(0, 0, 0.0))
}

fn economic(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::economic(pack, requested_w, at_ms, "plan")
}

#[test]
fn s261_no_hold_is_awaited_inside_the_tick() {
    let ladder = SafetyLadder::shared();
    for index in 0..4 {
        let pack = format!("P{index}");
        let mut input = SafetyInput::new(&pack, 0);
        if index == 0 {
            input.cell_latch = true;
        }
        ladder.observe(input);
    }
    let requests: Vec<SlotRequest> = (0..4)
        .map(|index| economic(&format!("P{index}"), -800.0, 0))
        .collect();
    let started = Instant::now();
    let interventions: Vec<_> = requests
        .iter()
        .map(|request| ladder.preempt(request, 0))
        .collect();
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_ne!(interventions[0], interventions[1]);
    let proof = open_proof(&ladder, "P0");
    assert!(!proof.verified);
    assert_eq!(proof.steps[0], "hold_issued");
}

#[test]
fn s262_solar_reserve_never_zeroes_a_sunless_night_grid_charge() {
    let mut input = SafetyInput::new("P", 0);
    input.targets.ceiling_kwh = 7.8575;
    input.soc_kwh = 7.131;
    input.soc_pct = 90.0;
    input.solar_reserve_kwh = 0.786;
    input.forecast_future_pv_kwh = 5.0;
    input.pv_w = Some(0.0);
    input.load_w = Some(500.0);
    let ladder = SafetyLadder::new();
    ladder.observe(input);

    let decision = ladder.decide(&economic("P", 3403.0, 0), 0);
    assert_eq!(decision.action, None);
    assert_eq!(decision.command_w, 0.0);
    assert_eq!(decision.solar.kwh, 0.0);
    assert_eq!(decision.solar.reason, "no_measured_pv_surplus");
    assert!(decision.steps.contains(&"no_measured_pv_surplus"));
    assert!(!decision.reasons.contains(&"solar_headroom_available"));
}

#[test]
fn s263_missing_or_stale_solar_telemetry_grants_nothing_without_blocking_grid_charge() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.load_w = None;
    ladder.observe(input.clone());
    let no_load = ladder.decide(&economic("P", 1000.0, 0), 0);
    assert_eq!(no_load.solar.kwh, 0.0);
    assert_eq!(no_load.solar.reason, "solar_load_telemetry_missing");
    assert_eq!(no_load.action, None);

    input.load_w = Some(500.0);
    input.pv_w = None;
    ladder.observe(input.clone());
    let no_pv = ladder.decide(&economic("P", 1000.0, 20_000), 20_000);
    assert_eq!(no_pv.solar.reason, "solar_telemetry_missing");
    assert_eq!(no_pv.action, None);

    input.pv_w = Some(3000.0);
    input.solar_sample_at_ms = Some(-31_000);
    ladder.observe(input);
    let stale = ladder.decide(&economic("P", 1000.0, 40_000), 40_000);
    assert_eq!(stale.solar.kwh, 0.0);
    assert_eq!(stale.solar.reason, "solar_telemetry_missing");
    assert_eq!(stale.action, None);
}

#[test]
fn s264_full_charge_pack_is_normal_and_never_charged_again() {
    let mut input = SafetyInput::new("P", 0);
    input.soc_pct = 100.0;
    input.fault_level = 2;
    input.is_error = false;
    let ladder = SafetyLadder::new();
    ladder.observe(input);

    for at_ms in [0, 20_000] {
        let decision = ladder.decide(&economic("P", 800.0, at_ms), at_ms);
        assert_eq!(decision.rule, energy_decide::safety::Rule::FullCharge);
        assert_eq!(decision.reason, "full_charge_protection");
        assert_eq!(
            decision.action,
            Some(energy_decide::dispatch::SafetyAction::NeutralHold)
        );
        assert!(
            !decision
                .reasons
                .iter()
                .any(|reason| reason.contains("fault"))
        );
        assert!(
            !decision
                .reasons
                .iter()
                .any(|reason| reason.contains("emergency_charge"))
        );
    }
}

#[test]
fn s270_stale_sample_ttl_stops_a_silent_reservation() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.last_sample_at_ms = -300_000;
    ladder.observe(input.clone());
    let fresh_enough = ladder.decide(&economic("P", 800.0, 0), 0);
    assert_eq!(fresh_enough.reason, "no_safety_hazard");

    input.last_sample_at_ms = -300_001;
    ladder.observe(input);
    let stale = ladder.decide(&economic("P", 800.0, 20_000), 20_000);
    assert_eq!(stale.reason, "stale_sample_unreachable");
    assert_eq!(
        stale.action,
        Some(energy_decide::dispatch::SafetyAction::NeutralHold)
    );
    assert!(ladder.hold_proof("P").is_some());
}
