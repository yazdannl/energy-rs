//! Live 2026-11-22..24 safety incidents, replayed at the M4 execution/report boundary.

mod m4_support;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use energy_decide::dispatch::{
    AllowAllAuthority, DispatchRow, DispatchState, Dispatcher, ProceedSafety, SafetyAction,
    SafetyGate, SafetyIntervention, SlotRequest,
};
use energy_decide::safety::{DecisionOutcome, SafetyInput, SafetyLadder, SafetyReport, from_row};
use m4_support::{Behavior, FakeIo, ImmediateClock};

struct SharedGate(Arc<SafetyLadder>);

impl SafetyGate for SharedGate {
    fn preempt(&self, request: &SlotRequest, now_ms: i64) -> SafetyIntervention {
        self.0.preempt(request, now_ms)
    }
}

struct ClampSafety {
    reason: &'static str,
}

impl SafetyGate for ClampSafety {
    fn preempt(&self, _request: &SlotRequest, _now_ms: i64) -> SafetyIntervention {
        SafetyIntervention::Hold {
            action: SafetyAction::NeutralHold,
            reason: self.reason,
        }
    }
}

struct DelayedSafety;

impl SafetyGate for DelayedSafety {
    fn preempt(&self, _request: &SlotRequest, _now_ms: i64) -> SafetyIntervention {
        thread::sleep(Duration::from_millis(300));
        SafetyIntervention::Hold {
            action: SafetyAction::NeutralHold,
            reason: "awaiting_hold",
        }
    }
}

struct FaultSafety;

impl SafetyGate for FaultSafety {
    fn preempt(&self, _request: &SlotRequest, _now_ms: i64) -> SafetyIntervention {
        SafetyIntervention::Hold {
            action: SafetyAction::EmergencyCharge { target_w: 800.0 },
            reason: "fault_level_2_charge",
        }
    }
}

fn economic(pack: &str, requested_w: f64, at_ms: i64) -> SlotRequest {
    SlotRequest::economic(pack, requested_w, at_ms, "plan")
}

fn accept(pack: &str) -> (&str, Behavior) {
    (pack, Behavior::Accept { echo: true })
}

fn run<G: SafetyGate>(
    gate: G,
    packs: &[(&str, Behavior)],
    requests: &[SlotRequest],
    now_ms: i64,
) -> (Vec<DispatchRow>, FakeIo) {
    let io = FakeIo::new(packs);
    let dut = Dispatcher::new(
        io.clone(),
        gate,
        AllowAllAuthority,
        ImmediateClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, requests, now_ms);
    (rows, io)
}

fn report(ladder: &SafetyLadder, pack: &str, row: &DispatchRow) -> SafetyReport {
    let decision = ladder
        .last_decision(pack)
        .unwrap_or_else(|| ladder.decide(&economic(pack, row.requested_w, 0), 0));
    from_row(row, &decision)
}

#[test]
fn s261_no_hold_is_awaited_inside_the_tick() {
    let ladder = Arc::new(SafetyLadder::new());
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
    let (rows, io) = run(
        SharedGate(Arc::clone(&ladder)),
        &[accept("P0"), accept("P1"), accept("P2"), accept("P3")],
        &requests,
        0,
    );
    assert!(started.elapsed() < Duration::from_millis(150));
    assert_eq!(rows.len(), 4);
    assert_eq!(io.device_w("P0"), Some(0.0));
    let decision = ladder
        .last_decision("P0")
        .unwrap_or_else(|| ladder.decide(&economic("P0", -800.0, 0), 0));
    assert_eq!(decision.retry_at_ms, Some(74_000));
    assert_eq!(decision.outcome, DecisionOutcome::Commanded);
    assert!(ladder.hold_proof("P0").is_some_and(|proof| !proof.verified));
}

#[test]
fn s262_solar_reserve_never_zeroes_a_sunless_night_grid_charge() {
    let ladder = Arc::new(SafetyLadder::new());
    let mut input = SafetyInput::new("P", 0);
    input.targets.ceiling_kwh = 7.8575;
    input.soc_kwh = 7.131;
    input.soc_pct = 90.0;
    input.solar_reserve_kwh = 0.786;
    input.forecast_future_pv_kwh = 5.0;
    input.pv_w = Some(0.0);
    input.load_w = Some(500.0);
    ladder.observe(input);

    let request = economic("P", 3403.0, 0);
    let (rows, io) = run(
        SharedGate(Arc::clone(&ladder)),
        &[accept("P")],
        std::slice::from_ref(&request),
        0,
    );
    assert_eq!(
        io.device_w("P"),
        Some(3403.0),
        "a downstream clamp must not zero the plan"
    );
    assert_eq!(rows[0].requested_w, 3403.0);
    assert!(rows[0].written);
    let report = report(&ladder, "P", &rows[0]);
    assert_eq!(report.decision.solar.kwh, 0.0);
    assert_eq!(report.decision.solar.reason, "no_measured_pv_surplus");
    assert!(!report.steps.contains(&"solar_headroom_available"));
}

#[test]
fn s263_missing_or_stale_solar_telemetry_grants_nothing_without_blocking_grid_charge() {
    let ladder = Arc::new(SafetyLadder::new());
    let mut input = SafetyInput::new("P", 0);
    input.load_w = None;
    ladder.observe(input.clone());
    let (no_load_rows, no_load_io) = run(
        SharedGate(Arc::clone(&ladder)),
        &[accept("P")],
        &[economic("P", 1000.0, 0)],
        0,
    );
    assert_eq!(no_load_io.device_w("P"), Some(1000.0));
    assert_eq!(
        report(&ladder, "P", &no_load_rows[0]).decision.solar.reason,
        "solar_load_telemetry_missing"
    );

    input.load_w = Some(500.0);
    input.pv_w = None;
    input.last_sample_at_ms = 20_000;
    input.grid_sample_at_ms = Some(20_000);
    ladder.observe(input.clone());
    let (no_pv_rows, no_pv_io) = run(
        SharedGate(Arc::clone(&ladder)),
        &[accept("P")],
        &[economic("P", 1000.0, 20_000)],
        20_000,
    );
    assert_eq!(no_pv_io.device_w("P"), Some(1000.0));
    assert_eq!(
        report(&ladder, "P", &no_pv_rows[0]).decision.solar.reason,
        "solar_telemetry_missing"
    );

    input.pv_w = Some(3000.0);
    input.solar_sample_at_ms = Some(-11_000);
    input.last_sample_at_ms = 40_000;
    input.grid_sample_at_ms = Some(40_000);
    ladder.observe(input);
    let (stale_rows, stale_io) = run(
        SharedGate(Arc::clone(&ladder)),
        &[accept("P")],
        &[economic("P", 1000.0, 40_000)],
        40_000,
    );
    assert_eq!(stale_io.device_w("P"), Some(1000.0));
    assert_eq!(
        report(&ladder, "P", &stale_rows[0]).decision.solar.reason,
        "solar_telemetry_missing"
    );
}

#[test]
fn s264_full_charge_pack_is_normal_and_never_charged_again() {
    let ladder = Arc::new(SafetyLadder::new());
    let mut input = SafetyInput::new("P", 0);
    input.soc_pct = 100.0;
    input.fault_level = 2;
    input.is_error = false;
    ladder.observe(input);

    for at_ms in [0, 20_000] {
        let pack = "P";
        let (rows, io) = run(
            SharedGate(Arc::clone(&ladder)),
            &[accept(pack)],
            &[economic(pack, 800.0, at_ms)],
            at_ms,
        );
        assert_eq!(io.device_w(pack), Some(0.0));
        assert_eq!(rows[0].requested_w, 0.0);
        let report = report(&ladder, pack, &rows[0]);
        assert_eq!(report.decision.reason, "full_charge_protection");
        assert!(!report.steps.iter().any(|step| step.contains("fault")));
        assert!(
            !report
                .steps
                .iter()
                .any(|step| step.contains("emergency_charge"))
        );
        assert!(
            report
                .decision
                .escalation
                .as_ref()
                .is_none_or(|escalation| escalation.steps.is_empty()
                    || !escalation.steps.iter().any(|step| step.contains("fault")))
        );
    }
}

#[test]
fn s270_stale_sample_ttl_stops_a_silent_reservation() {
    let ladder = SafetyLadder::new();
    let mut input = SafetyInput::new("P", 0);
    input.last_sample_at_ms = -39_000;
    ladder.observe(input.clone());
    let fresh_enough = ladder.decide(&economic("P", 800.0, 0), 0);
    assert_eq!(fresh_enough.reason, "no_safety_hazard");

    input.last_sample_at_ms = -300_001;
    ladder.observe(input);
    let stale = ladder.decide(&economic("P", 800.0, 20_000), 20_000);
    assert_eq!(stale.reason, "stale_sample_unreachable");
    assert_eq!(stale.action, Some(SafetyAction::NeutralHold));
    assert!(ladder.hold_proof("P").is_some());
}

#[test]
fn s260_mutation_proceeding_under_backoff_is_caught_at_the_device_boundary() {
    let mut input = SafetyInput::new("P", 0);
    input.grid_w = Some(7400.0);
    input.battery_w = Some(0.0);
    input.import_armed = true;
    input.backoff_until_ms = 300_000;
    let ladder = Arc::new(SafetyLadder::new());
    ladder.observe(input);
    let (rows, io) = run(
        ProceedSafety,
        &[accept("P")],
        &[economic("P", 3403.0, 0)],
        0,
    );
    assert_eq!(io.device_w("P"), Some(3403.0));
    assert!(rows[0].economic_preempted_by.is_none());
    assert!(
        !ladder
            .last_decision("P")
            .is_some_and(|decision| decision.reasons.contains(&"grid_import_ceiling"))
    );
}

#[test]
fn s261_mutation_waiting_for_a_hold_is_caught_by_the_tick_boundary() {
    let (rows, io) = run(
        DelayedSafety,
        &[accept("P")],
        &[economic("P", -800.0, 0)],
        0,
    );
    let started = Instant::now();
    let _ = run(
        DelayedSafety,
        &[accept("P")],
        &[economic("P", -800.0, 1)],
        1,
    );
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert_eq!(io.device_w("P"), Some(0.0));
    assert_eq!(rows[0].economic_preempted_by, Some("awaiting_hold"));
}

#[test]
fn s262_mutation_a_downstream_clamp_is_caught_by_device_power() {
    let (rows, io) = run(
        ClampSafety {
            reason: "solar_headroom_clamp",
        },
        &[accept("P")],
        &[economic("P", 3403.0, 0)],
        0,
    );
    assert_eq!(io.device_w("P"), Some(0.0));
    assert_eq!(rows[0].requested_w, 0.0);
    assert_eq!(rows[0].economic_preempted_by, Some("solar_headroom_clamp"));
}

#[test]
fn s263_mutation_missing_telemetry_clamp_is_caught_by_device_power() {
    let (rows, io) = run(
        ClampSafety {
            reason: "solar_telemetry_missing_clamp",
        },
        &[accept("P")],
        &[economic("P", 1000.0, 0)],
        0,
    );
    assert_eq!(io.device_w("P"), Some(0.0));
    assert_eq!(
        rows[0].economic_preempted_by,
        Some("solar_telemetry_missing_clamp")
    );
}

#[test]
fn s264_mutation_fault_escalation_is_caught_by_device_power() {
    let (rows, io) = run(FaultSafety, &[accept("P")], &[economic("P", 800.0, 0)], 0);
    assert_eq!(io.device_w("P"), Some(800.0));
    assert_eq!(rows[0].economic_preempted_by, Some("fault_level_2_charge"));
}
