// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{Command, ControlLimits, Direction, clamp_command, clamp_watts};
use serde_json::json;
use support::{SN, report, report_with};

#[test]
fn clamps_to_the_device_ceiling() {
    assert_eq!(clamp_watts(650, 0, 1000), 650);
    assert_eq!(clamp_watts(1400, 0, 1000), 1000);
    assert_eq!(clamp_watts(1400, 0, 800), 800);
}

#[test]
fn clamps_to_the_config_ceiling() {
    assert_eq!(clamp_watts(900, 500, 0), 500);
    assert_eq!(clamp_watts(900, 500, 800), 500);
    assert_eq!(clamp_watts(300, 500, 800), 300);
}

#[test]
fn an_unknown_ceiling_does_not_zero_the_power() {
    assert_eq!(clamp_watts(650, 0, 0), 650);
    assert_eq!(clamp_watts(650, 300, 0), 300);
}

#[test]
fn clamps_a_charge_command_to_the_live_charge_max() {
    let report = report(SN);
    let clamped = clamp_command(
        Command::Charge { watts: 1400 },
        &ControlLimits::unlimited(),
        Some(&report),
    );
    assert_eq!(clamped, Command::Charge { watts: 1000 });
}

#[test]
fn clamps_a_discharge_command_to_the_live_inverse_max() {
    let report = report(SN);
    let clamped = clamp_command(
        Command::Discharge { watts: 1400 },
        &ControlLimits::unlimited(),
        Some(&report),
    );
    assert_eq!(clamped, Command::Discharge { watts: 800 });
}

#[test]
fn config_and_device_ceilings_both_apply() {
    let report = report(SN);
    let limits = ControlLimits::new(500, 600);
    let charge = clamp_command(Command::Charge { watts: 1400 }, &limits, Some(&report));
    assert_eq!(charge, Command::Charge { watts: 500 });
    let discharge = clamp_command(Command::Discharge { watts: 1400 }, &limits, Some(&report));
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
    );
    assert_eq!(
        trim,
        Command::Trim {
            direction: Direction::Charge,
            watts: 1000
        }
    );
    assert_eq!(
        clamp_command(Command::Hold, &ControlLimits::new(1, 1), Some(&report)),
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
    );
    assert_eq!(clamped, Command::Charge { watts: 900 });
}
