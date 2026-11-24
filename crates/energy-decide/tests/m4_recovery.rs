//! Round-7 recovery regressions: shared prerequisites and ordered application.

mod m4_support;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use energy_decide::dispatch::{
    AllowAllAuthority, DispatchOutcome, DispatchState, Dispatcher, Echo, FlowReport, PackCommand,
    PackIo, PendingConfirmation, ProceedSafety, SlotRequest, WriteAck,
};
use m4_support::{Behavior, FakeIo, ImmediateClock, report};

fn economic(pack: &str, target_w: f64) -> SlotRequest {
    SlotRequest::economic(pack, target_w, 0, "recovery_test")
}

fn dispatcher(io: FakeIo) -> Dispatcher<ProceedSafety, AllowAllAuthority, ImmediateClock> {
    Dispatcher::new(
        io,
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        1_000,
    )
}

fn neutral_echo(pack: &str, at_ms: i64, message_id: u32) -> energy_decide::dispatch::FlowReport {
    let mut report = report(pack, at_ms, 0.0, 0, 0, message_id);
    report.echo = Some(Echo {
        limit_w: 0.0,
        at_ms,
    });
    report
}

fn uncertain_state(pack_id: &str, ordered_application: Option<bool>) -> DispatchState {
    let mut state = DispatchState::default();
    let pack = state.pack(pack_id);
    pack.uncertain = true;
    pack.transport_gone = true;
    pack.last_apply_attempt_ms = 100;
    pack.joined_at_ms = 100;
    pack.last_neutral_ack_at_ms = 200;
    pack.ordered_application = ordered_application;
    state
}

type TestDispatcher = Dispatcher<ProceedSafety, AllowAllAuthority, ImmediateClock>;

fn wait_outstanding(
    dut: &TestDispatcher,
    state: &mut DispatchState,
    now_ms: i64,
    want: usize,
) -> usize {
    let started = Instant::now();
    loop {
        dut.reconcile(state, now_ms);
        let observed = dut.outstanding_futures();
        if observed == want {
            return observed;
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "timed out after {elapsed:?} waiting for {want} outstanding futures; last observed {observed}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn s249_verified_hold_cannot_clear_while_transport_is_outstanding() {
    let io = FakeIo::new(&[(
        "P1",
        Behavior::Blocked {
            return_after_ms: 1_000,
        },
    )]);
    let dut = Dispatcher::new_with_shared(
        Arc::new(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        100,
    )
    .with_transport_bound_ms(50);
    let mut state = DispatchState::default();
    let first = dut.dispatch_slot(&mut state, &[economic("P1", -800.0)], 0);
    assert_eq!(first[0].outcome, DispatchOutcome::Uncertain);
    assert!(!state.pack("P1").transport_gone);
    state.pack("P1").pending = Some(PendingConfirmation::new(0.0, 100, Some(100), 45_000));

    let early = dut.observe(&mut state, &neutral_echo("P1", 200, 1), 200);
    let late = dut.observe(&mut state, &neutral_echo("P1", 300, 2), 300);
    assert!(late.reasons.contains(&"transport_still_outstanding"));
    assert!(state.pack("P1").uncertain);
    assert_eq!(state.pack("P1").uncertainty_cleared_by, None);
    assert_eq!(early.uncertainty_cleared_by, None);
    let _ = io.wait_for_tx_released(1, Duration::from_secs(2));
}

#[test]
fn s250_ordered_application_enables_causal_recovery() {
    let dut = dispatcher(FakeIo::new(&[]));
    let mut state = uncertain_state("P", Some(true));
    let row = dut.read_back(&mut state, &neutral_echo("P", 300, 1), 300);
    assert_eq!(row.uncertainty_cleared_by, Some("read_back"));
    assert!(!state.pack("P").uncertain);
}

#[test]
fn s250_without_ordered_application_needs_explicit_reconcile() {
    let dut = dispatcher(FakeIo::new(&[]));
    let mut state = uncertain_state("P", Some(false));
    let rejected = dut.read_back(&mut state, &neutral_echo("P", 300, 1), 300);
    assert!(rejected.reasons.contains(&"automatic_recovery_unsupported"));
    assert!(state.pack("P").uncertain);

    state.pack("P").pending = Some(PendingConfirmation::new(0.0, 100, Some(100), 45_000));
    let first = dut.observe(&mut state, &neutral_echo("P", 300, 2), 300);
    let second = dut.observe(&mut state, &neutral_echo("P", 400, 3), 400);
    assert_eq!(first.uncertainty_cleared_by, None);
    assert!(second.reasons.contains(&"automatic_recovery_unsupported"));
    assert!(state.pack("P").uncertain);

    state.pack("P").ordered_application = None;
    let absent = dut.read_back(&mut state, &neutral_echo("P", 500, 4), 500);
    assert!(absent.reasons.contains(&"automatic_recovery_unsupported"));
    assert!(state.pack("P").uncertain);

    dut.reconcile(&mut state, 600);
    assert!(!state.pack("P").uncertain);
    assert_eq!(
        state.pack("P").uncertainty_cleared_by,
        Some("explicit_reconcile")
    );
}

#[test]
fn s250_report_predating_the_hold_does_not_clear() {
    let dut = dispatcher(FakeIo::new(&[]));
    let mut state = uncertain_state("P", Some(true));
    let row = dut.read_back(&mut state, &neutral_echo("P", 150, 1), 300);
    assert!(row.reasons.contains(&"application_causality_unknown"));
    assert!(state.pack("P").uncertain);
    assert_eq!(state.pack("P").uncertainty_cleared_by, None);
}

#[test]
fn s248_device_clock_is_required_for_causality() {
    let dut = dispatcher(FakeIo::new(&[]));
    let mut state = uncertain_state("P", Some(true));
    let mut evidence = neutral_echo("P", 300, 1);
    evidence.device_clock_ok = false;
    let missing = dut.read_back(&mut state, &evidence, 300);
    assert!(missing.reasons.contains(&"application_causality_unknown"));
    assert!(state.pack("P").uncertain);

    evidence.device_clock_ok = true;
    evidence.device_clock_coherent = false;
    let incoherent = dut.read_back(&mut state, &evidence, 300);
    assert!(
        incoherent
            .reasons
            .contains(&"application_causality_unknown")
    );
    assert!(state.pack("P").uncertain);

    evidence.device_clock_coherent = true;
    let post_hold = dut.read_back(&mut state, &evidence, 300);
    assert_eq!(post_hold.uncertainty_cleared_by, Some("read_back"));
    assert!(!state.pack("P").uncertain);
}

#[test]
fn s250_port_capability_is_propagated_to_the_pack() {
    let io = FakeIo::new(&[("P", Behavior::Stuck { max_wait_ms: 50 })]);
    io.set_ordered_application(false);
    let dut = Dispatcher::new_with_shared(
        Arc::new(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        10,
    );
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    assert_eq!(state.pack("P").ordered_application, Some(false));
    std::thread::sleep(Duration::from_millis(60));
    let pack = state.pack("P");
    pack.transport_gone = true;
    pack.joined_at_ms = 60;
    let row = dut.read_back(&mut state, &neutral_echo("P", 100, 1), 100);
    assert!(row.reasons.contains(&"automatic_recovery_unsupported"));
    assert!(state.pack("P").uncertain);
    dut.reconcile(&mut state, 120);
    assert_eq!(
        state.pack("P").uncertainty_cleared_by,
        Some("explicit_reconcile")
    );
}

/// Two writes for one pack, each released by a separate channel. Both deliberately
/// ignore cancellation so reconciliation must decide from the futures that remain.
#[derive(Clone)]
struct ChannelIo {
    senders: Arc<Mutex<Vec<mpsc::Sender<WriteAck>>>>,
    next_tx: Arc<AtomicU64>,
}

impl ChannelIo {
    fn new() -> Self {
        Self {
            senders: Arc::new(Mutex::new(Vec::new())),
            next_tx: Arc::new(AtomicU64::new(0)),
        }
    }

    fn pending_count(&self) -> usize {
        self.senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    fn complete_first(&self, ack: WriteAck) {
        let sender = self
            .senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(0);
        let _ = sender.send(ack);
    }
}

impl PackIo for ChannelIo {
    fn begin(&self, _pack_id: &str) -> u64 {
        self.next_tx.fetch_add(1, Ordering::SeqCst)
    }

    fn release(&self, _pack_id: &str, _tx_id: u64) {}

    fn write(
        &self,
        _pack_id: &str,
        command: &PackCommand,
        _deadline_ms: u64,
        _tx_id: u64,
        _cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> WriteAck {
        let (sender, receiver) = mpsc::channel();
        self.senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(sender);
        receiver.recv().unwrap_or(WriteAck::Failed {
            reason: "channel_closed",
            at_ms: command.issued_at_ms,
        })
    }

    fn ordered_application(&self) -> bool {
        true
    }

    fn read(&self, _pack_id: &str, _deadline_ms: u64) -> Option<FlowReport> {
        None
    }
}

#[test]
fn s251_transport_gone_is_derived_from_remaining_writes() {
    let io = ChannelIo::new();
    let dut = Dispatcher::new_with_shared(
        Arc::new(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        20,
    );
    let mut state = DispatchState::default();
    let economic = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(economic[0].outcome, DispatchOutcome::Uncertain);
    let safety = dut.dispatch_slot(
        &mut state,
        &[SlotRequest::safety("P", 0.0, 100, "overlap_hold")],
        100,
    );
    assert_eq!(safety[0].outcome, DispatchOutcome::Uncertain);
    assert_eq!(io.pending_count(), 2);

    io.complete_first(WriteAck::Accepted {
        echo: Some(Echo {
            limit_w: 0.0,
            at_ms: 120,
        }),
        at_ms: 120,
    });
    assert_eq!(
        wait_outstanding(&dut, &mut state, 130, 1),
        1,
        "first write was never reaped"
    );
    assert!(
        !state.pack("P").transport_gone,
        "one reaped future must not mark a pack with another outstanding write as gone"
    );

    let rejected = dut.read_back(&mut state, &neutral_echo("P", 300, 1), 300);
    assert!(rejected.reasons.contains(&"transport_still_outstanding"));
    assert!(state.pack("P").uncertain);

    io.complete_first(WriteAck::Accepted {
        echo: Some(Echo {
            limit_w: 0.0,
            at_ms: 500,
        }),
        at_ms: 500,
    });
    assert_eq!(
        wait_outstanding(&dut, &mut state, 500, 0),
        0,
        "second write was never reaped"
    );
    assert!(state.pack("P").transport_gone);
    let cleared = dut.read_back(&mut state, &neutral_echo("P", 600, 2), 600);
    assert_eq!(cleared.uncertainty_cleared_by, Some("read_back"));
    assert!(!state.pack("P").uncertain);
}
