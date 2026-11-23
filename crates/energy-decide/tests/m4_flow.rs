//! M4 flow predicates, ramp timing and confirmation evidence (S24-S32, S82-S87,
//! S132, S133, S147, S151, S163, S170, S181, S188, S226).

mod m4_support;

use energy_decide::dispatch::constants::RAMP_MAX_MS;
use energy_decide::dispatch::{
    AllowAllAuthority, ConfirmationVerdict, DispatchOutcome, DispatchState, Dispatcher, FlowClass,
    FlowStatus, PendingConfirmation, ProceedSafety, fold_confirmation as fold, status_direction,
};
use m4_support::{FakeIo, ImmediateClock as TestClock, report};

type TestDispatcher = Dispatcher<ProceedSafety, AllowAllAuthority, TestClock>;

fn pending_state(pack: &str, command_w: f64) -> DispatchState {
    let mut state = DispatchState::default();
    state.pack(pack).pending = Some(PendingConfirmation::new(command_w, 0, Some(0), RAMP_MAX_MS));
    state
}

fn dispatcher() -> TestDispatcher {
    Dispatcher::new(
        FakeIo::default(),
        ProceedSafety,
        AllowAllAuthority,
        TestClock::default(),
        20_000,
    )
}

struct Case {
    command: f64,
    measured: f64,
    ac: i32,
    dc: i32,
    expected: FlowClass,
}

fn make_case(command: f64, m: f64, ac: i32, dc: i32, expected: FlowClass, negate: bool) -> Case {
    let measured = if negate { -m } else { m };
    Case {
        command,
        measured,
        ac,
        dc,
        expected,
    }
}

fn discharge_cases() -> Vec<Case> {
    use FlowClass as F;
    let rows = [
        (0.0, 0, 0, F::NoFlow),
        (5.0, 0, 0, F::NoFlow),
        (5.0, 9, 9, F::UnknownDirectionNoFlow),
        (5.0, 2, 1, F::OppositeDirectionNoFlow),
        (30.0, 9, 9, F::UnknownDirectionNoFlow),
        (31.0, 9, 9, F::UnknownDirectionFlow),
        (30.0, 2, 1, F::OppositeDirectionNoFlow),
        (31.0, 2, 1, F::WrongDirection),
        (80.0, 1, 2, F::PartialFlow),
        (80.0, 0, 0, F::IdleWithFlow),
        (80.0, 9, 9, F::UnknownDirectionFlow),
        (80.0, 2, 1, F::WrongDirection),
        (90.0, 1, 2, F::PartialFlow),
        (2880.0, 1, 2, F::FullFlow),
        (3200.0, 1, 2, F::FullFlow),
    ];
    rows.into_iter()
        .map(|(m, ac, dc, expected)| {
            make_case(
                -3200.0,
                m,
                ac,
                dc,
                expected,
                !matches!(expected, F::WrongDirection | F::OppositeDirectionNoFlow),
            )
        })
        .collect()
}

fn charge_cases(command: f64, over: FlowClass) -> Vec<Case> {
    use FlowClass as F;
    let rows = [
        (0.0, 0, 0, F::NoFlow),
        (5.0, 0, 0, F::NoFlow),
        (5.0, 9, 9, F::UnknownDirectionNoFlow),
        (5.0, 1, 2, F::OppositeDirectionNoFlow),
        (30.0, 9, 9, F::UnknownDirectionNoFlow),
        (31.0, 9, 9, F::UnknownDirectionFlow),
        (30.0, 1, 2, F::OppositeDirectionNoFlow),
        (31.0, 1, 2, F::WrongDirection),
        (80.0, 2, 1, F::PartialFlow),
        (80.0, 0, 0, F::IdleWithFlow),
        (80.0, 9, 9, F::UnknownDirectionFlow),
        (80.0, 1, 2, F::WrongDirection),
        (90.0, 2, 1, F::PartialFlow),
        (2880.0, 2, 1, over),
        (3200.0, 2, 1, over),
    ];
    rows.into_iter()
        .map(|(m, ac, dc, expected)| {
            make_case(
                command,
                m,
                ac,
                dc,
                expected,
                matches!(expected, F::WrongDirection | F::OppositeDirectionNoFlow),
            )
        })
        .collect()
}

fn small_charge_cases() -> Vec<Case> {
    use FlowClass as F;
    let rows = [
        (0.0, 0, 0, F::NoFlow),
        (5.0, 0, 0, F::NoFlow),
        (5.0, 9, 9, F::UnknownDirectionNoFlow),
        (5.0, 1, 2, F::OppositeDirectionNoFlow),
        (30.0, 9, 9, F::UnknownDirectionNoFlow),
        (31.0, 9, 9, F::UnknownDirectionFlow),
        (30.0, 1, 2, F::OppositeDirectionNoFlow),
        (31.0, 1, 2, F::WrongDirection),
        (30.0, 2, 1, F::NoFlow),
        (80.0, 2, 1, F::FullFlow),
        (80.0, 0, 0, F::IdleWithFlow),
        (80.0, 9, 9, F::UnknownDirectionFlow),
        (80.0, 1, 2, F::WrongDirection),
        (90.0, 2, 1, F::FullFlow),
        (2880.0, 2, 1, F::OverFlow),
        (3200.0, 2, 1, F::OverFlow),
    ];
    rows.into_iter()
        .map(|(m, ac, dc, expected)| {
            make_case(
                100.0,
                m,
                ac,
                dc,
                expected,
                matches!(expected, F::WrongDirection | F::OppositeDirectionNoFlow),
            )
        })
        .collect()
}

fn hold_cases() -> Vec<Case> {
    use FlowClass as F;
    let rows = [
        (80.0, 0, 0, F::IdleWithFlow),
        (80.0, 9, 9, F::UnknownDirectionFlow),
        (80.0, 2, 1, F::HoldUnexpectedFlow),
        (80.0, 1, 2, F::HoldUnexpectedFlow),
        (5.0, 0, 0, F::HoldVerified),
        (5.0, 9, 9, F::UnknownDirectionNoFlow),
        (5.0, 2, 1, F::HoldStatusDirectional),
        (5.0, 1, 2, F::HoldStatusDirectional),
        (30.0, 2, 1, F::HoldStatusDirectional),
        (31.0, 2, 1, F::HoldUnexpectedFlow),
        (30.0, 1, 2, F::HoldStatusDirectional),
        (31.0, 1, 2, F::HoldUnexpectedFlow),
        (30.0, 9, 9, F::UnknownDirectionNoFlow),
        (31.0, 9, 9, F::UnknownDirectionFlow),
    ];
    rows.into_iter()
        .map(|(m, ac, dc, expected)| make_case(0.0, m, ac, dc, expected, false))
        .collect()
}

fn cases() -> Vec<Case> {
    let mut rows = discharge_cases();
    rows.extend(charge_cases(800.0, FlowClass::OverFlow));
    rows.extend(small_charge_cases());
    rows.extend(hold_cases());
    rows
}

#[test]
fn s170_flow_predicate_truth_table() {
    use energy_decide::dispatch::classify_flow;
    for case in cases() {
        let status = status_direction(case.ac, case.dc);
        assert_eq!(
            classify_flow(case.command, case.measured, status),
            case.expected,
            "command {} measured {} status {:?}",
            case.command,
            case.measured,
            status
        );
        if case.expected.is_anomaly() {
            assert!(!case.expected.confirms());
        }
    }
    assert_eq!(status_direction(2, 1), FlowStatus::Charge);
    assert_eq!(status_direction(1, 2), FlowStatus::Discharge);
    assert_eq!(status_direction(0, 0), FlowStatus::Idle);
    assert_eq!(status_direction(-1, 0), FlowStatus::Invalid);
}

#[test]
fn s181_status_aware_small_flow_and_holds() {
    use energy_decide::dispatch::classify_flow;
    assert_eq!(
        classify_flow(-800.0, 5.0, FlowStatus::Unknown),
        FlowClass::UnknownDirectionNoFlow
    );
    assert_eq!(
        classify_flow(-800.0, 5.0, FlowStatus::Charge),
        FlowClass::OppositeDirectionNoFlow
    );
    assert_eq!(
        classify_flow(-800.0, 80.0, FlowStatus::Idle),
        FlowClass::IdleWithFlow
    );
    assert_eq!(
        classify_flow(0.0, 80.0, FlowStatus::Discharge),
        FlowClass::HoldUnexpectedFlow
    );
    assert_eq!(
        classify_flow(0.0, 5.0, FlowStatus::Discharge),
        FlowClass::HoldStatusDirectional
    );
    assert_eq!(
        classify_flow(0.0, 5.0, FlowStatus::Idle),
        FlowClass::HoldVerified
    );
}

#[test]
fn s226_ramp_ends_at_first_in_band_sample() {
    let mut pending = PendingConfirmation::new(800.0, 0, Some(0), RAMP_MAX_MS);
    let early = report("P", 20_000, 300.0, 2, 1, 1);
    assert_eq!(
        fold(&mut pending, &early, 20_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::PartialFlow,
            provisional: true
        }
    );
    let first = report("P", 36_000, 800.0, 2, 1, 2);
    assert_eq!(
        fold(&mut pending, &first, 36_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::FullFlow,
            provisional: false
        }
    );
    assert!(pending.ramp_ended());
    let second = report("P", 40_000, 799.0, 2, 1, 3);
    assert_eq!(
        fold(&mut pending, &second, 40_000, RAMP_MAX_MS),
        ConfirmationVerdict::Confirmed {
            class: FlowClass::FullFlow,
            measured_w: 799.0
        }
    );
}

#[test]
fn s084_s133_pre_ramp_evidence_is_provisional() {
    let mut pending = PendingConfirmation::new(-800.0, 0, Some(0), RAMP_MAX_MS);
    let ramp = report("P", 20_000, -300.0, 1, 2, 1);
    assert_eq!(
        fold(&mut pending, &ramp, 20_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::PartialFlow,
            provisional: true
        }
    );
    let first = report("P", 46_000, -790.0, 1, 2, 2);
    assert_eq!(
        fold(&mut pending, &first, 46_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::FullFlow,
            provisional: false
        }
    );
    let second = report("P", 49_000, -790.0, 1, 2, 3);
    assert!(matches!(
        fold(&mut pending, &second, 49_000, RAMP_MAX_MS),
        ConfirmationVerdict::Confirmed {
            class: FlowClass::FullFlow,
            ..
        }
    ));
}

#[test]
fn s082_s083_s188_hold_needs_idle_status() {
    let dut = dispatcher();
    let mut state = pending_state("P", 300.0);
    let first = report("P", 46_000, 299.0, 2, 1, 1);
    assert!(!dut.observe(&mut state, &first, 46_000).verified);
    let second = report("P", 49_000, 299.0, 2, 1, 2);
    let row = dut.observe(&mut state, &second, 49_000);
    assert_eq!(row.outcome, DispatchOutcome::Confirmed);
    assert!(row.verified);
    assert_eq!(row.applied_w, Some(299.0));

    let mut hold = pending_state("H", 0.0);
    let idle = report("H", 46_000, 5.0, 0, 0, 1);
    assert!(!dut.observe(&mut hold, &idle, 46_000).verified);
    let idle2 = report("H", 49_000, 0.0, 0, 0, 2);
    let hold_row = dut.observe(&mut hold, &idle2, 49_000);
    assert_eq!(hold_row.outcome, DispatchOutcome::Confirmed);
    assert_eq!(hold_row.applied_w, Some(0.0));
    assert!(hold_row.reasons.contains(&"hold_verified"));
}

#[test]
fn s087_s132_no_flow_needs_two_post_ramp_reports() {
    let mut pending = PendingConfirmation::new(500.0, 0, Some(0), RAMP_MAX_MS);
    let first = report("P", 46_000, 0.0, 0, 0, 1);
    assert_eq!(
        fold(&mut pending, &first, 46_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::NoFlow,
            provisional: false
        }
    );
    let second = report("P", 50_000, 0.0, 0, 0, 2);
    assert_eq!(
        fold(&mut pending, &second, 50_000, RAMP_MAX_MS),
        ConfirmationVerdict::Anomaly {
            class: FlowClass::NoFlow,
            measured_w: 0.0
        }
    );
    let mixed = report("P", 51_000, 299.0, 2, 1, 3);
    let mut mixed_pending = PendingConfirmation::new(300.0, 0, Some(0), RAMP_MAX_MS);
    assert_eq!(
        fold(&mut mixed_pending, &mixed, 46_000, RAMP_MAX_MS),
        ConfirmationVerdict::Pending {
            class: FlowClass::FullFlow,
            provisional: false
        }
    );
    let zero = report("P", 52_000, 0.0, 2, 1, 4);
    assert_eq!(
        fold(&mut mixed_pending, &zero, 47_000, RAMP_MAX_MS),
        ConfirmationVerdict::Anomaly {
            class: FlowClass::NoFlow,
            measured_w: 0.0
        }
    );
}

#[test]
fn s151_s163_relay_replay_confirms_only_on_later_ticks() {
    let dut = dispatcher();
    let mut state = pending_state("P", 800.0);
    let ramp = report("P", 36_000, 800.0, 2, 1, 1);
    let ramp_row = dut.observe(&mut state, &ramp, 36_000);
    assert!(!ramp_row.verified);
    assert!(ramp_row.applied_w.is_some());
    let later = report("P", 40_000, 800.0, 2, 1, 2);
    let row = dut.observe(&mut state, &later, 40_000);
    assert_eq!(row.outcome, DispatchOutcome::Confirmed);
    assert_eq!(row.applied_w, Some(800.0));
}

#[test]
fn s147_applied_power_is_never_written_or_intended_power() {
    let dut = dispatcher();
    let mut state = pending_state("P", 500.0);
    let report_zero = report("P", 46_000, 0.0, 0, 0, 1);
    let row = dut.observe(&mut state, &report_zero, 46_000);
    assert_eq!(row.applied_w, Some(0.0));
    assert_eq!(row.requested_w, 500.0);
}
