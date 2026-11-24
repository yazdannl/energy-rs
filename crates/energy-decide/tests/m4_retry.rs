//! M4 retry, pause and standing lifecycle scenarios (S24, S25, S88, S89,
//! S131, S148, S166).

mod m4_support;

use std::sync::Arc;
use std::time::Duration;

use energy_decide::dispatch::{
    AllowAllAuthority, DispatchOutcome, DispatchState, Dispatcher, PackIo, PendingConfirmation,
    ProceedSafety, SlotRequest, constants::RAMP_MAX_MS,
};
use m4_support::{Behavior, FakeIo, ImmediateClock, report};

fn shared(io: FakeIo) -> Arc<dyn PackIo> {
    Arc::new(io)
}

fn dut(io: FakeIo) -> Dispatcher<ProceedSafety, AllowAllAuthority, ImmediateClock> {
    Dispatcher::new_with_shared(
        shared(io),
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        20_000,
    )
}

#[test]
fn s089_fallback_hold_retries_at_0_2_6_seconds() {
    let io = FakeIo::new(&[("P", Behavior::FailFirst(3, "write_error"))]);
    let clock = ImmediateClock::default();
    let dispatcher = Dispatcher::new_with_shared(
        shared(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        clock.clone(),
        20_000,
    );
    let mut state = DispatchState::default();
    let request = SlotRequest::economic("P", -800.0, 0, "plan");
    let rows = dispatcher.dispatch_slot(&mut state, &[request], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    assert!(rows[0].reasons.contains(&"fallback_hold_written"));
    assert_eq!(rows[0].retry_offsets_ms, vec![0, 2_000, 6_000]);
    assert_eq!(clock.sleeps(), vec![2_000, 4_000]);
    assert_eq!(io.writes()[0].2, 20_000);
    assert!(
        io.writes()
            .iter()
            .skip(1)
            .all(|(_, command, _)| command.requested_w == 0.0)
    );
    assert!(io.wait_for_released(4, Duration::from_secs(2)));
    assert_eq!(io.active(), [] as [u64; 0]);
}

#[test]
fn s131_persistent_partial_escalates_after_pause() {
    let io = FakeIo::default();
    let dispatcher = dut(io);
    let mut state = DispatchState::default();
    state.pack("P").pending = Some(PendingConfirmation::new(-800.0, 0, Some(0), RAMP_MAX_MS));
    let partial = report("P", 46_000, -400.0, 1, 2, 1);
    let first = dispatcher.observe(&mut state, &partial, 46_000);
    assert!(first.anomaly_confirmed);
    assert!(first.reasons.contains(&"partial_flow"));
    assert_eq!(state.pack("P").pause_until_ms, 166_000);

    let mut after_pause = partial.clone();
    after_pause.message_id = 2;
    let second = dispatcher.observe(&mut state, &after_pause, 166_000);
    assert_eq!(second.outcome, DispatchOutcome::Deferred);
    assert!(second.reasons.contains(&"persistent_partial"));
    assert_eq!(second.retry_at, Some(286_000));

    let mut third = partial;
    third.message_id = 3;
    let escalated = dispatcher.observe(&mut state, &third, 286_000);
    assert!(escalated.reasons.contains(&"partial_flow_escalated"));
    assert!(state.pack("P").pending.is_some());
}

#[test]
fn s025_partial_pause_defers_economic_but_not_safety() {
    let io = FakeIo::default();
    let dispatcher = dut(io);
    let mut state = DispatchState::default();
    state.pack("P").pause_until_ms = 120_000;
    state.pack("P").backoff_reason = Some("partial_flow");
    let ordinary = dispatcher.dispatch_slot(
        &mut state,
        &[SlotRequest::economic("P", -800.0, 0, "plan")],
        30_000,
    );
    assert_eq!(ordinary[0].outcome, DispatchOutcome::Deferred);
    assert_eq!(ordinary[0].retry_at, Some(120_000));
    let safety = dispatcher.dispatch_slot(
        &mut state,
        &[SlotRequest::safety("P", 0.0, 0, "cell_voltage_guard")],
        30_000,
    );
    assert_ne!(safety[0].outcome, DispatchOutcome::Deferred);
    assert!(safety[0].safety_traffic);
}

#[test]
fn s088_standing_lifecycle_requested_ack_confirmed() {
    let io = FakeIo::default();
    let dispatcher = dut(io);
    let mut state = DispatchState::default();
    let request = SlotRequest::economic("P", -800.0, 0, "plan");
    let ack = dispatcher.dispatch_slot(&mut state, &[request], 0);
    assert_eq!(ack[0].written_w, Some(-800.0));
    assert!(!ack[0].verified);
    let first = report("P", 46_000, -790.0, 1, 2, 1);
    let second = report("P", 49_000, -790.0, 1, 2, 2);
    let pending_row = dispatcher.observe(&mut state, &first, 46_000);
    let verified = dispatcher.observe(&mut state, &second, 49_000);
    assert!(!pending_row.verified);
    assert_eq!(verified.outcome, DispatchOutcome::Confirmed);
    assert_eq!(verified.confirmed_w, Some(-800.0));
    assert_eq!(verified.applied_w, Some(-790.0));
    assert!(state.pack("P").pending.is_none());
}

#[test]
fn s166_backoff_boundary_is_inclusive() {
    let io = FakeIo::default();
    let dispatcher = dut(io);
    let mut state = DispatchState::default();
    state.pack("P").backoff_until_ms = 300_000;
    state.pack("P").backoff_reason = Some("no_flow");
    let early = dispatcher.dispatch_slot(
        &mut state,
        &[SlotRequest::economic("P", -800.0, 0, "plan")],
        299_999,
    );
    assert_eq!(early[0].outcome, DispatchOutcome::Deferred);
    let at = dispatcher.dispatch_slot(
        &mut state,
        &[SlotRequest::economic("P", -800.0, 0, "plan")],
        300_000,
    );
    assert_ne!(at[0].outcome, DispatchOutcome::Deferred);
}

#[test]
fn s024_no_flow_arms_300s_backoff() {
    let io = FakeIo::default();
    let dispatcher = dut(io);
    let mut state = DispatchState::default();
    state.pack("P").pending = Some(PendingConfirmation::new(500.0, 0, Some(0), RAMP_MAX_MS));
    let a = report("P", 46_000, 0.0, 0, 0, 1);
    let b = report("P", 47_000, 0.0, 0, 0, 2);
    let first = dispatcher.observe(&mut state, &a, 46_000);
    let second = dispatcher.observe(&mut state, &b, 47_000);
    assert!(!first.anomaly_confirmed);
    assert!(second.anomaly_confirmed);
    assert_eq!(state.pack("P").backoff_until_ms, 347_000);
    assert_eq!(state.pack("P").backoff_reason, Some("no_flow"));
}

#[test]
fn s148_every_row_has_an_outcome_and_a_reason() {
    let io = FakeIo::new(&[
        ("A", Behavior::Accept { echo: true }),
        ("B", Behavior::Reject("device_refused")),
        ("C", Behavior::Accept { echo: false }),
    ]);
    let dispatcher = dut(io);
    let mut state = DispatchState::default();
    let requests: Vec<SlotRequest> = ["A", "B", "C"]
        .iter()
        .map(|p| SlotRequest::economic(p, -800.0, 0, "plan"))
        .collect();
    for row in dispatcher.dispatch_slot(&mut state, &requests, 0) {
        assert!(row.reasons.iter().any(|reason| !reason.is_empty()));
        assert!(!matches!(row.outcome, DispatchOutcome::Confirmed));
    }
}

#[test]
fn s090_deadband_and_ramp_timing() {
    use energy_decide::dispatch::constants::{HOLD_DEADBAND_W, RAMP_MAX_MS, is_hold};
    assert_eq!(HOLD_DEADBAND_W, 30.0);
    assert_eq!(RAMP_MAX_MS, 45_000);
    assert!(is_hold(-29.0));
    assert!(!is_hold(-31.0));
    let mut pending = PendingConfirmation::new(-800.0, 0, Some(0), RAMP_MAX_MS);
    let report_at_45 = report("P", 45_000, -29.0, 0, 0, 1);
    let row = dut(FakeIo::default()).observe(&mut state_with(&mut pending), &report_at_45, 45_000);
    assert!(row.reasons.contains(&"no_flow") || row.anomaly_confirmed);
}

fn state_with(pending: &mut PendingConfirmation) -> DispatchState {
    let mut state = DispatchState::default();
    state.pack("P").pending = Some(pending.clone());
    state
}

#[test]
fn s091_changed_target_interrupts_partial_pause() {
    let dispatcher = dut(FakeIo::default());
    let mut state = DispatchState::default();
    let pack = state.pack("P");
    pack.pause_until_ms = 120_000;
    pack.pause_target_w = Some(-800.0);
    let rows = dispatcher.dispatch_slot(
        &mut state,
        &[SlotRequest::economic("P", 800.0, 30_000, "plan")],
        30_000,
    );
    assert_eq!(rows[0].outcome, DispatchOutcome::ConfirmationPending);
    assert!(
        rows[0]
            .reasons
            .contains(&"pause_interrupted_by_target_change")
    );
    assert_eq!(state.pack("P").pause_until_ms, 0);
    assert_eq!(state.pack("P").pause_target_w, None);
}
