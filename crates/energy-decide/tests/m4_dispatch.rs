//! M4 incidents S221-S225 and consumer-port/row observability S227-S233.
mod m4_support;
use energy_decide::dispatch::constants::PACK_IO_BUDGET_MS;
use energy_decide::dispatch::{
    AllowAllAuthority, AppliedReason, AuthorityDecision, DispatchOutcome, DispatchState,
    Dispatcher, Echo, PackCommand, PackIo, ProceedSafety, SafetyAction, SafetyExecution,
    SafetyGate, SafetyIntervention, SlotRequest, WriteAuthority,
};
use m4_support::{Behavior, FakeIo, ImmediateClock as TestClock, report};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
struct HoldSafety;
impl SafetyGate for HoldSafety {
    fn preempt(&self, _request: &SlotRequest, _now_ms: i64) -> SafetyIntervention {
        SafetyIntervention::Hold {
            action: SafetyAction::NeutralHold,
            reason: "cell_voltage_guard",
        }
    }
}

struct ProtectionActiveAuthority;
impl WriteAuthority for ProtectionActiveAuthority {
    fn authorize(&self, _command: &PackCommand) -> AuthorityDecision {
        AuthorityDecision::Denied {
            reason: "protection_active",
        }
    }
}

struct DenyAuthority;
impl WriteAuthority for DenyAuthority {
    fn authorize(&self, _command: &PackCommand) -> AuthorityDecision {
        AuthorityDecision::Denied {
            reason: "write_denied_by_authority_gate",
        }
    }
}

fn shared(io: FakeIo) -> Arc<dyn PackIo> {
    Arc::new(io)
}

fn economic(pack: &str, watts: f64) -> SlotRequest {
    SlotRequest::economic(pack, watts, 0, "plan")
}
#[test]
fn s221_flow_window_is_not_awaited_inside_the_tick() {
    let dut = Dispatcher::new(
        FakeIo::default(),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let requests: Vec<SlotRequest> = ["P1", "P2", "P3", "P4"]
        .iter()
        .map(|p| economic(p, -800.0))
        .collect();
    let started = Instant::now();
    let rows = dut.dispatch_slot(&mut state, &requests, 0);
    assert!(started.elapsed().as_millis() < 2_000);
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .all(|row| row.outcome == DispatchOutcome::ConfirmationPending)
    );
    assert!(rows.iter().all(|row| row.applied_w.is_none()));
    assert!(
        rows.iter()
            .all(|row| row.reasons.contains(&"flow_not_measured"))
    );
}
#[test]
fn s222_unbounded_transport_is_bounded_and_transaction_released() {
    let io = FakeIo::new(&[
        ("P1", Behavior::Accept { echo: true }),
        ("P2", Behavior::Accept { echo: true }),
        ("P3", Behavior::Never),
        ("P4", Behavior::Accept { echo: true }),
    ]);
    let dut = Dispatcher::<_, _, _>::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        500,
    );
    let mut state = DispatchState::default();
    let requests: Vec<SlotRequest> = ["P1", "P2", "P3", "P4"]
        .iter()
        .map(|p| economic(p, -800.0))
        .collect();
    let started = Instant::now();
    let rows = dut.dispatch_slot(&mut state, &requests, 0);
    assert!(started.elapsed().as_millis() < 2_000);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[2].outcome, DispatchOutcome::Uncertain);
    assert_eq!(
        rows[2].reasons,
        vec![
            "write_deadline_mid_transport",
            "controller_uncertain",
            "fallback_budget_exhausted"
        ]
    );
    assert_eq!(io.write_count(), 4);
    assert!(io.wait_for_released(4, Duration::from_secs(2)));
    assert_eq!(io.active(), [] as [u64; 0]);
    assert_eq!(io.released().len(), 4);
    assert!(io.peak_active() <= 4);
    assert!(rows.iter().filter(|r| r.pack_id != "P3").all(|r| r.written));
}
#[test]
fn s223_deadline_row_is_never_written_or_accepted() {
    let io = FakeIo::new(&[("P3", Behavior::Never)]);
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        120,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P3", -800.0)], 0);
    let row = &rows[0];
    assert_eq!(row.outcome, DispatchOutcome::Uncertain);
    assert!(!row.written);
    assert!(!row.verified);
    assert_eq!(row.applied_w, None);
    assert_eq!(row.applied_reason, AppliedReason::TransportDeadline);
    assert!(row.controller_uncertain);
}
#[test]
fn s225_uncertainty_blocks_retries_until_verified_hold() {
    let io = FakeIo::new(&[("P3", Behavior::Never)]);
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        120,
    );
    let mut state = DispatchState::default();
    let request = economic("P3", -800.0);
    let _ = dut.dispatch_slot(&mut state, std::slice::from_ref(&request), 0);
    let retry = dut.dispatch_slot(&mut state, std::slice::from_ref(&request), 1_000);
    assert_eq!(retry[0].outcome, DispatchOutcome::Deferred);
    assert!(retry[0].reasons.contains(&"controller_uncertain"));
    assert_eq!(io.write_count(), 1);
    thread::sleep(Duration::from_millis(20));
    dut.reconcile(&mut state, 1_500);

    let mut hold_report = report("P3", 2_000, 0.0, 0, 0, 1);
    hold_report.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 2_000,
    });
    state.pack("P3").pending = Some(energy_decide::dispatch::PendingConfirmation::new(
        0.0,
        1_000,
        Some(1_000),
        45_000,
    ));
    let first = dut.observe(&mut state, &hold_report, 2_000);
    let mut second_report = hold_report.clone();
    second_report.message_id = 2;
    second_report.at_ms = 9_000;
    second_report.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 9_000,
    });
    let second = dut.observe(&mut state, &second_report, 9_000);
    assert!(first.reasons.contains(&"hold_verified"));
    assert_eq!(second.outcome, DispatchOutcome::Confirmed);
    assert_eq!(second.uncertainty_cleared_by, Some("verified_hold"));
    assert!(!state.pack("P3").uncertain);
    assert_eq!(
        state.pack("P3").uncertainty_cleared_by,
        Some("verified_hold")
    );
}
#[test]
fn s224_low_soc_refusal_is_capability_shortfall_not_a_fault() {
    let io = FakeIo::new(&[("P3", Behavior::Accept { echo: true })]);
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P3", -565.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::ConfirmationPending);
    let mut refused = report("P3", 46_000, 0.0, 0, 0, 1);
    refused.soc_pct = 31.0;
    refused.device_min_soc_pct = 10.0;
    refused.fault_level = 0;
    let row = dut.observe(&mut state, &refused, 46_000);
    assert_eq!(row.outcome, DispatchOutcome::CapabilityShortfall);
    assert_eq!(row.applied_w, Some(0.0));
    assert_eq!(row.delivered_w, Some(0.0));
    assert_eq!(row.shortfall_requested_w, Some(-565.0));
    assert!(row.reasons.contains(&"discharge_derated_at_soc"));
    assert!(!row.fault_backoff_incremented);
    assert_eq!(state.pack("P3").backoff_until_ms, 0);
}
#[test]
fn s227_one_budget_is_observable_in_the_row() {
    let dut = Dispatcher::production(
        FakeIo::default(),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P1", -800.0)], 0);
    assert_eq!(PACK_IO_BUDGET_MS, 20_000);
    assert_eq!(rows[0].budget_ms, PACK_IO_BUDGET_MS);
    assert!(rows[0].elapsed_ms <= rows[0].budget_ms);
}
#[test]
fn s229_safety_refusal_is_obeyed_and_recorded() {
    let io = FakeIo::default();
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        HoldSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P1", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::ConfirmationPending);
    assert!(rows[0].safety_traffic);
    assert_eq!(rows[0].economic_preempted_by, Some("cell_voltage_guard"));
    assert_eq!(rows[0].safety_execution, Some(SafetyExecution::Echoed));
    assert!(rows[0].written);
    assert!(rows[0].reasons.contains(&"cell_voltage_guard"));
    let writes = io.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].1.requested_w, 0.0);
}
#[test]
fn s230_authority_denial_is_not_retried_around() {
    let io = FakeIo::default();
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        DenyAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let request = economic("P1", -800.0);
    let first = dut.dispatch_slot(&mut state, std::slice::from_ref(&request), 0);
    let second = dut.dispatch_slot(&mut state, &[request], 1_000);
    assert_eq!(first[0].outcome, DispatchOutcome::Skipped);
    assert!(!first[0].written);
    assert!(first[0].reasons.contains(&"write_denied_by_authority_gate"));
    assert_eq!(io.write_count(), 0);
    assert_eq!(second[0].outcome, DispatchOutcome::Skipped);
    assert_eq!(io.write_count(), 0);
}
#[test]
fn s232_echo_is_a_gate_not_a_verdict() {
    let io = FakeIo::new(&[
        ("P1", Behavior::Accept { echo: false }),
        ("P2", Behavior::Accept { echo: true }),
    ]);
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(
        &mut state,
        &[economic("P1", -800.0), economic("P2", -800.0)],
        0,
    );
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    assert!(rows[0].written);
    assert!(rows[0].applied_w.is_none());
    assert_eq!(rows[0].applied_reason, AppliedReason::EchoMissing);
    assert_eq!(rows[1].outcome, DispatchOutcome::ConfirmationPending);
    assert!(!rows[1].verified);
    assert_eq!(rows[1].applied_reason, AppliedReason::FlowNotMeasured);
}
#[test]
fn s231_s233_rows_distinguish_every_state() {
    let io = FakeIo::new(&[
        ("A", Behavior::Accept { echo: true }),
        ("B", Behavior::Reject("device_refused")),
        ("C", Behavior::Accept { echo: false }),
        ("D", Behavior::Accept { echo: true }),
    ]);
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let requests = ["A", "B", "C", "D"]
        .iter()
        .map(|p| economic(p, -800.0))
        .collect::<Vec<_>>();
    let mut rows = dut.dispatch_slot(&mut state, &requests, 0);
    rows.truncate(3);
    let mut hold = report("D", 46_000, 0.0, 0, 0, 1);
    hold.soc_pct = 31.0;
    rows.push(dut.observe(&mut state, &hold, 46_000));
    let signatures = rows
        .iter()
        .map(|r| {
            (
                r.outcome,
                r.applied_reason,
                r.applied_w,
                r.written,
                r.evidence_kind,
            )
        })
        .collect::<Vec<_>>();
    for i in 0..signatures.len() {
        for j in (i + 1)..signatures.len() {
            assert_ne!(
                signatures[i], signatures[j],
                "rows {i} and {j} must be distinguishable"
            );
        }
    }
    let read = dut.read_flow(&mut state, "A", 47_000, 5_000);
    assert_eq!(read.applied_reason, AppliedReason::ReadMiss);
    assert!(read.applied_w.is_none());
}
#[test]
fn s041_null_applied_w_is_typed_not_zero() {
    let io = FakeIo::new(&[("A", Behavior::Accept { echo: true })]);
    let dut = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("A", 500.0)], 0);
    assert_eq!(rows[0].applied_w, None);
    assert_eq!(rows[0].applied_reason, AppliedReason::FlowNotMeasured);
}
#[test]
fn s043_s092_protection_active_skips_economic_writes() {
    let io = FakeIo::default();
    let dispatcher = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        ProtectionActiveAuthority,
        TestClock::default(),
        20_000,
    );
    let mut state = DispatchState::default();
    let rows = dispatcher.dispatch_slot(&mut state, &[economic("P1", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Skipped);
    assert_eq!(rows[0].applied_w, None);
    assert!(rows[0].reasons.contains(&"protection_active"));
    assert_eq!(io.write_count(), 0);
}
#[test]
fn s247_uncertain_exits_by_read_back() {
    let io = FakeIo::new(&[("P", Behavior::Never)]);
    let dut = Dispatcher::new_with_shared(
        Arc::new(io),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        120,
    );
    let mut state = DispatchState::default();
    let request = economic("P", -800.0);
    let timed_out = dut.dispatch_slot(&mut state, std::slice::from_ref(&request), 0);
    assert_eq!(timed_out[0].outcome, DispatchOutcome::Uncertain);
    thread::sleep(Duration::from_millis(300));
    dut.reconcile(&mut state, 1_000);
    assert!(state.pack("P").uncertain);
    assert_eq!(state.pack("P").uncertainty_cleared_by, None);

    let mut first = report("P", 1_001, 0.0, 0, 0, 1);
    first.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 1_001,
    });
    let cleared = dut.read_back(&mut state, &first, 1_001);
    assert_eq!(cleared.uncertainty_cleared_by, Some("read_back"));
    assert!(!state.pack("P").uncertain);
}
#[test]
fn s247_uncertain_exits_by_verified_hold() {
    let io = FakeIo::default();
    let dut = Dispatcher::new_with_shared(
        Arc::new(io),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        120,
    );
    let mut first = report("P", 1_001, 0.0, 0, 0, 1);
    first.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 1_001,
    });
    let mut second = first.clone();
    second.at_ms = 7_002;
    second.message_id = 2;
    second.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 7_002,
    });
    let mut held = DispatchState::default();
    held.pack("P").uncertain = true;
    held.pack("P").uncertain_reason = Some("write_deadline_mid_transport");
    held.pack("P").last_apply_attempt_ms = 2_000;
    held.pack("P").ordered_application = Some(true);
    held.pack("P").pending = Some(energy_decide::dispatch::PendingConfirmation::new(
        0.0,
        1_000,
        Some(1_000),
        45_000,
    ));
    let hold_first = dut.observe(&mut held, &first, 1_001);
    assert!(hold_first.reasons.contains(&"hold_verified"));
    assert!(held.pack("P").uncertain);
    let mut close = first.clone();
    close.at_ms = 1_002;
    close.message_id = 2;
    close.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 1_002,
    });
    let early_hold = dut.observe(&mut held, &close, 1_002);
    assert!(early_hold.reasons.contains(&"hold_verified"));
    assert!(held.pack("P").uncertain);
    second.message_id = 3;
    let _hold_second = dut.observe(&mut held, &second, 7_002);
    assert!(held.pack("P").uncertain);
    let mut third = second.clone();
    third.at_ms = 7_003;
    third.message_id = 4;
    third.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 7_003,
    });
    let hold_second = dut.observe(&mut held, &third, 7_003);
    assert_eq!(hold_second.uncertainty_cleared_by, Some("verified_hold"));
    assert!(!held.pack("P").uncertain);
}
