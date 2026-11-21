//! Proof that a command is moving power, not merely that the pack echoed its limits.
//!
//! A 200 OK and a matching `inputLimit`/`outputLimit` only say the RAM register changed. On the
//! 800 Plus the AC relay can stay open while those registers read back perfectly (incident
//! 2026-11-21: `acStatus 0`, `dcStatus 0`, `gridInputPower 0` for minutes). Grid charging is proved
//! by `gridInputPower` (or `outputPackPower`) rising with the verified `acStatus 2`/`dcStatus 1`
//! mapping. Discharge is proved by `outputHomePower` alone: this fleet has never discharged, so
//! treating a guessed status mapping as mandatory could latch every pack out and remove the grid
//! import ceiling. Statuses are corroboration and an alarm there, never a discharge gate.

use std::time::Duration;

use crate::control::command::Command;
use crate::control::direction::Direction;
use crate::report::Report;

/// Absolute floor below which measured watts are indistinguishable from idle noise.
pub const FLOW_THRESHOLD_W: i64 = 5;

/// A running pack may legitimately taper far below its command (45/48/35 W steps were observed).
/// Require 10 % of the command, with the absolute floor, so low curtailment still counts as flow.
const COMMAND_FRACTION_PCT: i64 = 10;

/// A single flowing report can be a transient. Require consecutive fresh reports with flow;
/// the device reports about once a second, so two samples are a real (short) time window.
pub const FLOW_SAMPLES: u8 = 2;

/// The AC relay takes 20-36 s to come up on a cold pack; an 8 s echo window declares success
/// before any power can appear. Direction changes get a window that covers the whole ramp.
pub const FLOW_CONFIRM_WINDOW: Duration = Duration::from_secs(45);

/// How much power must be measured before this command can be considered alive, W.
#[must_use]
pub fn flow_threshold(commanded_w: f64) -> i64 {
    let watts = commanded_w.abs().round() as i64;
    (watts * COMMAND_FRACTION_PCT / 100).max(FLOW_THRESHOLD_W)
}

/// The same floor for callers that already hold an integer watt command.
#[must_use]
pub fn flow_threshold_w(commanded_w: u32) -> i64 {
    flow_threshold(f64::from(commanded_w))
}

/// Measured power in the command's direction, W (never the commanded limit).
#[must_use]
pub fn measured_w(command: &Command, report: &Report) -> i64 {
    let properties = &report.properties;
    match command.direction() {
        Some(Direction::Charge) => properties.grid_input_w.max(properties.output_pack_w),
        Some(Direction::Discharge) => properties.output_home_w,
        None => 0,
    }
}

/// What one report says about a command's real power flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowEvidence {
    /// The commanded direction is measurably carrying power.
    Flowing,
    /// Some power is moving but below this command's floor: wait, do not fault.
    Curtailed,
    /// No power at all (or the verified charge status is absent): a failed start.
    Absent,
}

impl FlowEvidence {
    /// Whether this report is flow proof.
    #[must_use]
    pub const fn is_flowing(self) -> bool {
        matches!(self, Self::Flowing)
    }

    /// Whether this report proves a failed start, as opposed to legitimate low output.
    #[must_use]
    pub const fn is_absent(self) -> bool {
        matches!(self, Self::Absent)
    }
}

/// Classify one report against a command, without ever treating an echo as power.
#[must_use]
pub fn assess(command: &Command, report: &Report) -> FlowEvidence {
    let Some(direction) = command.direction() else {
        return FlowEvidence::Absent;
    };
    let measured = measured_w(command, report);
    if measured <= FLOW_THRESHOLD_W {
        // 1-4 W is idle noise, not a start: never turn it into a partial confirmation.
        return FlowEvidence::Absent;
    }
    // Charge's status mapping was verified on hardware (acStatus 2, dcStatus 1, 299 W). Keep it
    // strict; discharge has no hardware evidence and must not be gated on an assumed mapping.
    if direction == Direction::Charge
        && (report.properties.ac_status != Some(2) || report.properties.dc_status != Some(1))
    {
        return FlowEvidence::Absent;
    }
    if measured < flow_threshold_w(command.watts().unwrap_or_default()) {
        FlowEvidence::Curtailed
    } else {
        FlowEvidence::Flowing
    }
}

/// Whether one report proves the commanded direction is actually carrying power.
#[must_use]
pub fn is_flowing(command: &Command, report: &Report) -> bool {
    assess(command, report).is_flowing()
}
