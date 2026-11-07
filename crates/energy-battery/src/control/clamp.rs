//! Clamp a command to the config ceiling and the device's live maxima.

use crate::control::command::Command;
use crate::control::direction::Direction;
use crate::control::limits::ControlLimits;
use crate::report::Report;

/// Requested watts clamped to a config ceiling and a device ceiling.
///
/// 0 means "unknown" on both sides: a device that does not report `chargeMaxLimit` must not be read
/// as "no power", and an unset config limit must not clamp everything to 0. When both are unknown
/// the request passes through.
pub fn clamp_watts(requested: u32, config_max: u32, device_max: u32) -> u32 {
    let ceiling = match (config_max, device_max) {
        (0, 0) => u32::MAX,
        (0, device) => device,
        (config, 0) => config,
        (config, device) => config.min(device),
    };
    requested.min(ceiling)
}

/// Clamp a command against the config ceilings and the last report's live maxima.
///
/// A command without a report (first tick) is clamped against the config only; the controller
/// refuses to write before a report exists, so this is a pure helper for planning/tests.
pub fn clamp_command(command: Command, limits: &ControlLimits, report: Option<&Report>) -> Command {
    let (Some(direction), Some(watts)) = (command.direction(), command.watts()) else {
        return command;
    };
    let device_max = report.map_or(0, |report| device_max(direction, report));
    let clamped = clamp_watts(watts, limits.for_direction(direction), device_max);
    command.with_watts(clamped)
}

fn device_max(direction: Direction, report: &Report) -> u32 {
    match direction {
        Direction::Charge => report.properties.charge_max_w,
        Direction::Discharge => report.properties.inverse_max_w,
    }
}
