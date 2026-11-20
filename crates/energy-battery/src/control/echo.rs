//! Confirmation by echo: the only success signal the device gives.

use std::time::Duration;

use crate::control::command::Command;
use crate::control::direction::Direction;
use crate::control::fault::ControlFault;
use crate::control::pending::Pending;
use crate::report::Report;

/// The vendor's `POWER_TOLERANCE`: a limit counts as applied within 5 W.
pub const ECHO_TOLERANCE_W: u32 = 5;

/// How long a fresh report with the *old* limits may leave a write in doubt.
///
/// The 800 Plus emits reports continuously while it applies a write: measured on Kees
/// (192.168.1.86) on 2026-11-18, a `-788 W` write was acknowledged at 20:18:23, reports at
/// 20:18:23-27 still showed `outputLimit: 0`, and `outputLimit: 788` appeared at 20:18:28. The
/// limit then stayed at 788 for ~9 s while `outputHomePower` ramped to 787 W. The window covers
/// **limit application** (~5 s observed), not the ~8-10 s power ramp that follows it. Eight seconds
/// leaves a 3 s margin over the measured lag, and the heartbeat keeps it under the engine's 15 s
/// watchdog stall bound. Safety holds use the much shorter
/// [`SAFETY_HOLD_WINDOW`](crate::control::controller::SAFETY_HOLD_WINDOW).
pub const ECHO_CONFIRM_WINDOW: Duration = Duration::from_secs(8);

/// Spacing of read-backs while a write is in doubt (the device reports about once a second).
pub const ECHO_POLL_INTERVAL: Duration = Duration::from_millis(500);

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

/// Whether `report` is a *new* observation of the device rather than a duplicate or an old one.
///
/// Limit values alone cannot prove a command landed: an idle report from before the write already
/// shows `inputLimit`/`outputLimit` at 0, so it would "confirm" a hold that never took. A report can
/// only speak about a command when it is strictly newer than the report that was current when the
/// command went out:
///
/// * the 8-bit `messageId` must have moved forward on the ring (a repeated counter is the same
///   report, not evidence);
/// * the device clock must not have gone back;
/// * when the write ack carried a `messageId` of its own, the report must not be behind it.
///
/// The counter is the strongest signal the protocol offers; a duplicate fails at the first check.
///
/// Freshness is *not* application: the SolarFlow800 Plus keeps emitting reports while a write is
/// being applied. A report can therefore have a newer `messageId` and still carry the pre-write
/// limits (measured on hardware; see [`ECHO_CONFIRM_WINDOW`]).
/// Such a report is kept as a doubt by [`is_echo_doubt`], and a later fresh report with the new
/// limit confirms.
///
/// `sequence` is the 8-bit counter unwrapped by [`Inner`](crate::control::state::Inner) across every
/// report observed since the baseline. Comparing sequences instead of the raw counter is safe
/// across any number of 255→0 wraps: a replay has a sequence at or below the newest report already
/// consumed for this command, and a later report always above it. The assumption is that consecutive *observed* reports never jump more
/// than [`RESTART_WINDOW`](crate::control::watch::RESTART_WINDOW) counter steps — at the measured ~1-4 reports/s that is minutes, and a
/// larger jump is already treated as a restart by [`watch`](crate::control::watch). The device clock
/// must also not go backwards (its one-second resolution cannot order reports within a second).
/// Nothing here accepts a duplicate or a foreign report, and the wait is bounded.
pub fn is_fresh(pending: &Pending, report: &Report, sequence: u64) -> bool {
    let (_, baseline_ts) = pending.baseline;
    if sequence <= pending.last_seen_sequence {
        return false;
    }
    if report.timestamp != 0 && report.timestamp < baseline_ts {
        return false;
    }
    pending.acked_sequence.is_none_or(|acked| sequence >= acked)
}

/// Whether a mismatch is only a limit echo that may still be in flight.
///
/// Wedge, restart, foreign and wrong-direction reports are safety signals and must keep failing
/// immediately; only the limit values are given the bounded tolerance.
pub fn is_echo_doubt(fault: &ControlFault) -> bool {
    matches!(
        fault,
        ControlFault::EchoMismatch { .. } | ControlFault::HoldEcho { .. }
    )
}

/// Forward distance between two 8-bit report counters.
pub(super) fn forward(from: u32, to: u32) -> u32 {
    to.wrapping_sub(from) & 0xFF
}
