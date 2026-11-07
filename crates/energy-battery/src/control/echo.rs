//! Confirmation by echo: the only success signal the device gives.

use crate::control::command::Command;
use crate::control::direction::Direction;
use crate::control::fault::ControlFault;
use crate::report::Report;

/// The vendor's `POWER_TOLERANCE`: a limit counts as applied within 5 W.
pub const ECHO_TOLERANCE_W: u32 = 5;

/// Check the *next* report against a command.
///
/// The limit echo is the signal; `acMode` is checked when the device reports a usable direction, and
/// skipped when it reports 0 (a firmware that does not fill the field must not look like a fault).
pub fn verify(command: &Command, report: &Report) -> Result<(), ControlFault> {
    match *command {
        Command::Hold => verify_hold(report),
        Command::Charge { watts } => verify_direction(Direction::Charge, watts, report),
        Command::Discharge { watts } => verify_direction(Direction::Discharge, watts, report),
        Command::Trim { direction, watts } => verify_direction(direction, watts, report),
    }
}

fn verify_direction(
    direction: Direction,
    expected_w: u32,
    report: &Report,
) -> Result<(), ControlFault> {
    let ac_mode = report.properties.ac_mode;
    if let Some(reported) = Direction::from_ac_mode(ac_mode)
        && reported != direction
    {
        return Err(ControlFault::WrongMode {
            sn: report.sn.clone(),
            expected: direction,
            ac_mode,
        });
    }
    let echoed_w = direction.limit(&report.properties);
    if echoed_w.abs_diff(expected_w) > ECHO_TOLERANCE_W {
        return Err(ControlFault::EchoMismatch {
            sn: report.sn.clone(),
            direction,
            expected_w,
            echoed_w,
        });
    }
    Ok(())
}

fn verify_hold(report: &Report) -> Result<(), ControlFault> {
    let input_w = report.properties.input_limit_w;
    let output_w = report.properties.output_limit_w;
    if input_w > ECHO_TOLERANCE_W || output_w > ECHO_TOLERANCE_W {
        return Err(ControlFault::HoldEcho {
            sn: report.sn.clone(),
            input_w,
            output_w,
        });
    }
    Ok(())
}
