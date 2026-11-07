// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{
    BatteryError, Ceiling, Command, ControlLimits, Direction, HardLimit, MAX_HARD_LIMIT_W,
    ceiling_for, clamp_command, clamp_watts,
};
use serde_json::json;
use support::{SN, report, report_with};

#[test]
fn clamps_to_the_device_ceiling() {
    assert_eq!(clamp_watts(650, Ceiling::Known(1000)), Some(650));
    assert_eq!(clamp_watts(1400, Ceiling::Known(1000)), Some(1000));
    assert_eq!(clamp_watts(1400, Ceiling::Known(800)), Some(800));
}

#[test]
fn clamps_to_the_config_ceiling() {
    let report = report(SN);
    let limits = ControlLimits::new(500, 500);
    assert_eq!(
        clamp_command(Command::Charge { watts: 900 }, &limits, Some(&report)).unwrap(),
        Command::Charge { watts: 500 }
    );
    assert_eq!(
        clamp_command(Command::Charge { watts: 300 }, &limits, Some(&report)).unwrap(),
        Command::Charge { watts: 300 }
    );
}

#[test]
fn an_unknown_ceiling_refuses_a_nonzero_request() {
    // H13: `(config_max, device_max) == (0, 0)` used to select `u32::MAX`, so an unbounded command
    // was transmitted as if "unknown" meant "unlimited".
    assert_eq!(clamp_watts(650, Ceiling::Unknown), None);
    assert_eq!(Ceiling::from_watts(0), Ceiling::Unknown);
    assert_eq!(Ceiling::from_watts(1), Ceiling::Known(1));
}

#[test]
fn a_zero_watt_request_needs_no_ceiling() {
    assert_eq!(clamp_watts(0, Ceiling::Unknown), Some(0));
    assert_eq!(clamp_watts(0, Ceiling::Known(1000)), Some(0));
}

#[test]
fn a_known_ceiling_beats_an_unknown_one() {
    assert_eq!(
        Ceiling::Unknown.tighter(Ceiling::Known(400)),
        Ceiling::Known(400)
    );
    assert_eq!(
        Ceiling::Known(400).tighter(Ceiling::Known(900)),
        Ceiling::Known(400)
    );
    assert_eq!(Ceiling::Unknown.tighter(Ceiling::Unknown), Ceiling::Unknown);
}

#[test]
fn clamps_a_charge_command_to_the_live_charge_max() {
    let report = report(SN);
    let clamped = clamp_command(
        Command::Charge { watts: 1400 },
        &ControlLimits::unlimited(),
        Some(&report),
    )
    .unwrap();
    assert_eq!(clamped, Command::Charge { watts: 1000 });
}

#[test]
fn clamps_a_discharge_command_to_the_live_inverse_max() {
    let report = report(SN);
    let clamped = clamp_command(
        Command::Discharge { watts: 1400 },
        &ControlLimits::unlimited(),
        Some(&report),
    )
    .unwrap();
    assert_eq!(clamped, Command::Discharge { watts: 800 });
}

#[test]
fn config_and_device_ceilings_both_apply() {
    let report = report(SN);
    let limits = ControlLimits::new(500, 600);
    let charge = clamp_command(Command::Charge { watts: 1400 }, &limits, Some(&report)).unwrap();
    assert_eq!(charge, Command::Charge { watts: 500 });
    let discharge =
        clamp_command(Command::Discharge { watts: 1400 }, &limits, Some(&report)).unwrap();
    assert_eq!(discharge, Command::Discharge { watts: 600 });
}

#[test]
fn trims_clamp_the_same_way_and_holds_do_not_change() {
    let report = report(SN);
    let trim = clamp_command(
        Command::Trim {
            direction: Direction::Charge,
            watts: 2000,
        },
        &ControlLimits::new(0, 0),
        Some(&report),
    )
    .unwrap();
    assert_eq!(
        trim,
        Command::Trim {
            direction: Direction::Charge,
            watts: 1000
        }
    );
    assert_eq!(
        clamp_command(Command::Hold, &ControlLimits::new(1, 1), Some(&report)).unwrap(),
        Command::Hold
    );
}

#[test]
fn a_device_without_live_maxima_keeps_the_config_ceiling() {
    let report = report_with(SN, json!({"chargeMaxLimit": 0, "inverseMaxPower": 0}));
    let clamped = clamp_command(
        Command::Charge { watts: 1400 },
        &ControlLimits::new(900, 900),
        Some(&report),
    )
    .unwrap();
    assert_eq!(clamped, Command::Charge { watts: 900 });
}

#[test]
fn a_device_without_any_ceiling_is_an_error_not_a_pass_through() {
    let report = report_with(SN, json!({"chargeMaxLimit": 0, "inverseMaxPower": 0}));
    let error = clamp_command(
        Command::Charge { watts: 1400 },
        &ControlLimits::unlimited(),
        Some(&report),
    )
    .unwrap_err();
    match error {
        BatteryError::NoCeiling {
            direction,
            requested_w,
            ..
        } => {
            assert_eq!(direction, Direction::Charge);
            assert_eq!(requested_w, 1400);
        }
        other => panic!("expected NoCeiling, got {other:?}"),
    }
    assert_eq!(
        ceiling_for(
            &ControlLimits::unlimited(),
            Some(&report),
            Direction::Charge
        ),
        Ceiling::Unknown
    );
}

#[test]
fn a_validated_hard_limit_bounds_a_device_that_reports_nothing() {
    let silent = report_with(SN, json!({"chargeMaxLimit": 0, "inverseMaxPower": 0}));
    let hard = HardLimit::new(1200, 900).unwrap();
    let limits = ControlLimits::unlimited().with_hard(Some(hard));
    assert_eq!(
        clamp_command(Command::Charge { watts: 2000 }, &limits, Some(&silent)).unwrap(),
        Command::Charge { watts: 1200 }
    );
    assert_eq!(
        clamp_command(Command::Discharge { watts: 2000 }, &limits, Some(&silent)).unwrap(),
        Command::Discharge { watts: 900 }
    );
    // The live maximum still wins when it is the tighter of the two.
    assert_eq!(
        clamp_command(Command::Charge { watts: 2000 }, &limits, Some(&report(SN))).unwrap(),
        Command::Charge { watts: 1000 }
    );
}

#[test]
fn a_hard_limit_outside_the_validated_band_is_refused() {
    assert!(HardLimit::new(0, 800).is_err());
    assert!(HardLimit::new(800, 0).is_err());
    assert!(HardLimit::new(MAX_HARD_LIMIT_W + 1, 800).is_err());
    assert!(HardLimit::new(MAX_HARD_LIMIT_W, 800).is_ok());
}
