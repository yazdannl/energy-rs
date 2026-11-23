//! M4 review row regressions: two safety facts and post-ramp shortfall only.

mod m4_support;

use energy_decide::dispatch::constants::RAMP_MAX_MS;
use energy_decide::dispatch::{
    AllowAllAuthority, ConfirmationVerdict, DispatchOutcome, DispatchState, Dispatcher, FlowClass,
    PendingConfirmation, SafetyAction, SafetyExecution, SafetyGate, SafetyIntervention,
    SlotRequest, fold_confirmation as fold,
};
use m4_support::{Behavior, FakeIo, ImmediateClock, report};

struct HoldSafety;
impl SafetyGate for HoldSafety {
    fn preempt(&self, _request: &SlotRequest, _now_ms: i64) -> SafetyIntervention {
        SafetyIntervention::Hold {
            action: SafetyAction::NeutralHold,
            reason: "cell_voltage_guard",
        }
    }
}

fn held_dispatch(
    io: FakeIo,
    budget_ms: u64,
) -> Dispatcher<HoldSafety, AllowAllAuthority, ImmediateClock> {
    Dispatcher::new(
        io,
        HoldSafety,
        AllowAllAuthority,
        ImmediateClock::default(),
        budget_ms,
    )
}

fn economic(pack: &str, watts: f64) -> SlotRequest {
    SlotRequest::economic(pack, watts, 0, "plan")
}

#[test]
fn s236_clean_safety_hold_keeps_decision_and_execution_apart() {
    let dut = held_dispatch(FakeIo::default(), 20_000);
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].economic_preempted_by, Some("cell_voltage_guard"));
    assert_eq!(rows[0].safety_execution, Some(SafetyExecution::Echoed));
    assert_eq!(rows[0].outcome, DispatchOutcome::ConfirmationPending);
    assert!(rows[0].written);
    assert!(rows[0].echo_matched);
}

#[test]
fn s236_timed_out_safety_hold_is_uncertain_not_skipped() {
    let io = FakeIo::new(&[("P", Behavior::Never)]);
    let dut = held_dispatch(io, 100);
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Uncertain);
    assert_ne!(rows[0].outcome, DispatchOutcome::Skipped);
    assert_eq!(rows[0].economic_preempted_by, Some("cell_voltage_guard"));
    assert_eq!(rows[0].safety_execution, Some(SafetyExecution::TimedOut));
    assert_eq!(rows[0].safety_reason, Some("write_deadline_mid_transport"));
    assert!(!rows[0].written);
    assert!(rows[0].controller_uncertain);
}

#[test]
fn s236_rejected_safety_hold_is_rejected_not_skipped() {
    let io = FakeIo::new(&[("P", Behavior::Reject("safety_refused"))]);
    let dut = held_dispatch(io.clone(), 20_000);
    let mut state = DispatchState::default();
    let rows = dut.dispatch_slot(&mut state, &[economic("P", -800.0)], 0);
    assert_eq!(rows[0].outcome, DispatchOutcome::Rejected);
    assert_ne!(rows[0].outcome, DispatchOutcome::Skipped);
    assert_eq!(rows[0].economic_preempted_by, Some("cell_voltage_guard"));
    assert_eq!(rows[0].safety_execution, Some(SafetyExecution::Rejected));
    assert_eq!(rows[0].safety_reason, Some("safety_refused"));
    assert!(!rows[0].written);
    assert_eq!(io.write_count(), 1);
}

fn low_soc_report(
    at_ms: i64,
    measured_w: f64,
    ac: i32,
    dc: i32,
    id: u32,
) -> energy_decide::dispatch::FlowReport {
    let mut value = report("P", at_ms, measured_w, ac, dc, id);
    value.soc_pct = 31.0;
    value.device_min_soc_pct = 10.0;
    value.fault_level = 0;
    value
}

#[test]
fn s224_charge_ramp_below_target_is_not_a_shortfall() {
    let mut pending = PendingConfirmation::new(800.0, 0, Some(0), RAMP_MAX_MS);
    let ramp = low_soc_report(20_000, 100.0, 2, 1, 1);
    assert_eq!(
        fold(&mut pending, &ramp, 20_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::PartialFlow,
            provisional: true
        }
    );
    let stopped = low_soc_report(46_000, 0.0, 0, 0, 2);
    assert!(!matches!(
        fold(&mut pending, &stopped, 46_000, RAMP_MAX_MS),
        ConfirmationVerdict::Shortfall { .. }
    ));
}

#[test]
fn s224_discharge_ramp_sample_is_provisional_not_shortfall() {
    let mut pending = PendingConfirmation::new(-800.0, 0, Some(0), RAMP_MAX_MS);
    let ramp = low_soc_report(20_000, -200.0, 1, 2, 1);
    assert_eq!(
        fold(&mut pending, &ramp, 20_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::PartialFlow,
            provisional: true
        }
    );
    let stopped = low_soc_report(46_000, 0.0, 0, 0, 2);
    assert_eq!(
        fold(&mut pending, &stopped, 46_000, RAMP_MAX_MS),
        ConfirmationVerdict::Shortfall { measured_w: 0.0 }
    );
}
