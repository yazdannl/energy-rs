//! Transport-contract violations: latched packs and honest deadlines.

mod m4_support;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use energy_decide::dispatch::{
    AllowAllAuthority, DispatchOutcome, DispatchState, Dispatcher, Echo, ProceedSafety, SlotRequest,
};
use m4_support::{Behavior, FakeIo, ImmediateClock, report};

fn economic(pack: &str, target_w: f64) -> SlotRequest {
    SlotRequest::economic(pack, target_w, 0, "contract_test")
}

#[test]
fn s222_transport_violation_latches_one_pack_until_reconcile() {
    let io = FakeIo::new(&[
        (
            "P1",
            Behavior::Blocked {
                return_after_ms: 500,
            },
        ),
        ("P2", Behavior::Accept { echo: true }),
        ("P3", Behavior::Accept { echo: true }),
        ("P4", Behavior::Accept { echo: true }),
    ]);
    let dut = Dispatcher::new_with_shared(
        Arc::new(io.clone()),
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        1_000,
    )
    .with_transport_bound_ms(50);
    let mut state = DispatchState::default();
    let requests: Vec<SlotRequest> = ["P1", "P2", "P3", "P4"]
        .iter()
        .map(|pack| economic(pack, -800.0))
        .collect();
    let rows = dut.dispatch_slot(&mut state, &requests, 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    assert!(rows[0].reasons.contains(&"transport_contract_violated"));
    assert!(rows.iter().skip(1).all(|r| r.written));
    assert!(state.pack("P1").transport_contract_violated);
    assert!(!state.pack("P2").transport_contract_violated);

    let blocked = dut.dispatch_slot(&mut state, &[economic("P1", -600.0)], 100);
    assert_eq!(blocked[0].outcome, DispatchOutcome::Deferred);
    assert!(blocked[0].reasons.contains(&"transport_contract_violated"));
    assert_eq!(io.write_count(), 4);

    let _ = io.wait_for_tx_released(1, Duration::from_secs(2));
    thread::sleep(Duration::from_millis(20));
    dut.reconcile(&mut state, 200);
    assert!(!state.pack("P1").transport_contract_violated);
    io.set_behavior("P1", Behavior::Accept { echo: true });
    let mut proof = report("P1", 250, 0.0, 0, 0, 1);
    proof.echo = Some(Echo {
        limit_w: 0.0,
        at_ms: 250,
    });
    let cleared = dut.read_back(&mut state, &proof, 250);
    assert_eq!(cleared.uncertainty_cleared_by, Some("read_back"));
    let allowed = dut.dispatch_slot(&mut state, &[economic("P1", -600.0)], 300);
    assert!(allowed[0].written);
}

#[test]
fn s237_write_just_over_transport_bound_trips_violation() {
    let io = FakeIo::new(&[(
        "P",
        Behavior::Blocked {
            return_after_ms: 120,
        },
    )]);
    let dut = Dispatcher::new(
        io,
        ProceedSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        1_000,
    )
    .with_transport_bound_ms(100);
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    assert!(rows[0].reasons.contains(&"transport_contract_violated"));
    assert_eq!(
        rows[0].applied_reason,
        energy_decide::dispatch::AppliedReason::TransportContractViolated
    );
}
