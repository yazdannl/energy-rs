//! M4 review lifecycle regressions: absolute deadlines, generations and read misses.

mod m4_support;

use std::thread;
use std::time::{Duration, Instant};

use energy_decide::dispatch::{
    AllowAllAuthority, AppliedReason, DispatchOutcome, DispatchState, Dispatcher, Echo,
    ProceedSafety, SlotRequest,
};
use m4_support::{Behavior, FakeIo, ImmediateClock, report};

fn dispatcher(
    io: FakeIo,
    budget_ms: u64,
) -> Dispatcher<ProceedSafety, AllowAllAuthority, ImmediateClock> {
    Dispatcher::new(
        io,
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        budget_ms,
    )
}

fn economic(pack: &str, watts: f64) -> SlotRequest {
    SlotRequest::economic(pack, watts, 0, "plan")
}

fn wait_for_workers(io: &FakeIo, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if io.active_workers() == 0 {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    false
}

fn wait_for_release(io: &FakeIo, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if io.active().is_empty() {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    false
}

#[test]
fn s222_s237_absolute_deadlines_release_every_transaction_at_deadline() {
    let io = FakeIo::new(&[
        ("P1", Behavior::Stuck { max_wait_ms: 5_000 }),
        ("P2", Behavior::Stuck { max_wait_ms: 5_000 }),
        ("P3", Behavior::Stuck { max_wait_ms: 5_000 }),
        ("P4", Behavior::Stuck { max_wait_ms: 5_000 }),
    ]);
    let dut = dispatcher(io.clone(), 300);
    let mut state = DispatchState::default();
    let requests: Vec<SlotRequest> = ["P1", "P2", "P3", "P4"]
        .iter()
        .map(|p| economic(p, -800.0))
        .collect();
    let started = Instant::now();
    let rows = dut.dispatch_slot(&mut state, &requests, 0);
    let elapsed = started.elapsed().as_millis();
    assert!(
        elapsed < 450,
        "fleet took {elapsed} ms, more than one budget"
    );
    assert!(
        rows.iter()
            .all(|row| row.outcome == DispatchOutcome::Uncertain)
    );
    assert!(wait_for_release(&io, Duration::from_millis(300)));
    assert_eq!(io.peak_active(), 4);
    let latency = io.release_latency_ms();
    assert_eq!(latency.len(), 4);
    assert!(
        latency.iter().all(|ms| *ms <= 400),
        "late release: {latency:?}"
    );
    let high = latency.iter().copied().max().unwrap_or(0);
    let low = latency.iter().copied().min().unwrap_or(u64::MAX);
    let spread = high.saturating_sub(low);
    assert!(spread < 100, "releases were serial: {latency:?}");
}

#[test]
fn s235_late_completion_cannot_overwrite_a_later_hold() {
    let io = FakeIo::new(&[("P", Behavior::Late { after_ms: 300 })]);
    let dut = dispatcher(io.clone(), 80);
    let mut state = DispatchState::default();
    let first = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(first[0].outcome, DispatchOutcome::Uncertain);
    assert!(first[0].reasons.contains(&"write_deadline_mid_transport"));
    io.set_behavior("P", Behavior::Accept { echo: true });
    let hold = SlotRequest::safety("P", 0.0, 1_000, "later_hold");
    let second = dut.dispatch_slot(&mut state, &[hold], 1_000);
    assert!(second[0].written);
    thread::sleep(Duration::from_millis(420));
    let late = dut.take_late_writes();
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].pack_id, "P");
    assert_eq!(late[0].generation, 1);
    assert_eq!(late[0].reason, "late_after_deadline_superseded");
    let pending = state.pack("P").pending.as_ref();
    assert!(pending.is_some());
    assert_eq!(pending.map_or(1.0, |value| value.command_w), 0.0);
    assert_eq!(state.pack("P").generation, 3);
    let writes = io.writes();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes.last().map_or(1.0, |value| value.1.requested_w), 0.0);
}

#[test]
fn s235_consecutive_deadlines_do_not_accumulate_workers() {
    let io = FakeIo::new(&[("P", Behavior::Stuck { max_wait_ms: 2_000 })]);
    let dut = dispatcher(io.clone(), 60);
    let mut state = DispatchState::default();
    for round in 0..5 {
        let started = Instant::now();
        let safety = SlotRequest::safety("P", -800.0, round * 1_000, "bounded_retry");
        let rows = dut.dispatch_slot(&mut state, &[safety], round * 1_000);
        let elapsed = started.elapsed().as_millis();
        assert!(elapsed < 200, "round {round} took {elapsed} ms");
        assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
        assert!(wait_for_release(&io, Duration::from_millis(300)));
        assert!(wait_for_workers(&io, Duration::from_millis(500)));
        assert_eq!(io.active_workers(), 0);
        assert_eq!(io.peak_workers(), 1);
    }
    assert_eq!(io.released().len(), 5);
}

#[test]
fn s225_read_miss_after_timeout_keeps_uncertainty_and_command_identity() {
    let io = FakeIo::new(&[("P", Behavior::Never)]);
    let dut = dispatcher(io, 80);
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    let read = dut.read_flow(&mut state, "P", 1_000, 1_000);
    assert_eq!(read.outcome, DispatchOutcome::Uncertain);
    assert_eq!(read.requested_w, -800.0);
    assert_eq!(read.generation, 1);
    assert_eq!(read.applied_reason, AppliedReason::ReadMiss);
    assert!(read.controller_uncertain);
    assert!(read.reasons.contains(&"controller_uncertain"));
    assert!(!read.written);
}

#[test]
fn s225_read_miss_without_pending_command_is_not_a_confirmation() {
    let dut = dispatcher(FakeIo::default(), 80);
    let mut state = DispatchState::default();
    let read = dut.read_flow(&mut state, "P", 1_000, 1_000);
    assert_eq!(read.outcome, DispatchOutcome::Deferred);
    assert!(read.reasons.contains(&"no_pending_command"));
    assert_eq!(read.applied_reason, AppliedReason::ReadMiss);
    assert_eq!(read.requested_w, 0.0);
    assert!(!read.written);
    assert!(!read.controller_uncertain);
}

#[test]
fn s238_blocked_first_does_not_supersede_fast_followers() {
    let io = FakeIo::new(&[
        ("P1", Behavior::Stuck { max_wait_ms: 5_000 }),
        ("P2", Behavior::Accept { echo: true }),
        ("P3", Behavior::Accept { echo: true }),
        ("P4", Behavior::Accept { echo: true }),
    ]);
    let dut = dispatcher(io, 250);
    let mut state = DispatchState::default();
    let requests: Vec<SlotRequest> = ["P1", "P2", "P3", "P4"]
        .iter()
        .map(|p| economic(p, -800.0))
        .collect();
    let started = Instant::now();
    let rows = dut.dispatch_slot(&mut state, &requests, 0);
    assert!(started.elapsed().as_millis() < 400);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    for row in rows.iter().skip(1) {
        assert_eq!(row.outcome, DispatchOutcome::ConfirmationPending);
        assert!(row.written);
        assert!(!row.reasons.contains(&"late_after_deadline_superseded"));
        assert!(!row.reasons.contains(&"write_deadline_mid_transport"));
    }
}

#[test]
fn s239_worker_termination_bound_is_counted() {
    let io = FakeIo::new(&[("P", Behavior::Late { after_ms: 250 })]);
    let dut = dispatcher(io.clone(), 80);
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    thread::sleep(Duration::from_millis(420));
    let violations = dut.take_worker_violations();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].pack_id, "P");
    assert_eq!(violations[0].bound_ms, 160);
    assert!(violations[0].outstanding >= 1);
    assert_eq!(dut.take_late_writes().len(), 1);
    assert!(wait_for_workers(&io, Duration::from_millis(300)));
}

#[test]
fn s240_late_device_effect_requires_truth_before_the_next_command() {
    let io = FakeIo::new(&[("P", Behavior::Late { after_ms: 150 })]);
    let dut = dispatcher(io.clone(), 80);
    let mut state = DispatchState::default();
    let timed_out = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(timed_out[0].outcome, DispatchOutcome::Uncertain);
    let blocked = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 90);
    assert_eq!(blocked[0].outcome, DispatchOutcome::Deferred);
    assert_eq!(io.applied_commands().len(), 0);

    thread::sleep(Duration::from_millis(300));
    let still_blocked = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 400);
    assert_eq!(still_blocked[0].outcome, DispatchOutcome::Deferred);
    assert_eq!(io.applied_commands().len(), 1);
    assert_eq!(io.device_w("P"), Some(-800.0));

    let mut readback = report("P", 410, 0.0, 0, 0, 1);
    readback.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 410,
    });
    let read = dut.read_back(&mut state, &readback, 410);
    assert_eq!(read.uncertainty_cleared_by, Some("read_back"));
    assert!(!state.pack("P").uncertain);
    assert_eq!(state.pack("P").uncertainty_cleared_by, Some("read_back"));

    io.set_behavior("P", Behavior::Accept { echo: true });
    let neutral = dut.dispatch_slot(&mut state, &[economic("P", 0.0)], 420);
    assert!(neutral[0].written);
    assert_eq!(io.applied_commands().len(), 2);
    assert_eq!(io.device_w("P"), Some(0.0));
}
