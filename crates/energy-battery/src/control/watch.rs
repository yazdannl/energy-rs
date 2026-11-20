//! Wedge and report-ordering detection on the report stream.
//!
//! Ordering is classified *before* any accepted counter state changes. A report that cannot be
//! ordered (a replay, a large jump, a reset with an equal or older device clock) is inconclusive: it
//! neither confirms nor faults a command, and it does not move the accepted sequence forward.

use crate::control::fault::ControlFault;
use crate::report::Report;

const CHARGING: i32 = 1;
const DISCHARGING: i32 = 2;
/// `messageId` counts in 8 bits; a jump of more than half the ring is not a forward wrap.
pub(super) const RESTART_WINDOW: u32 = 128;
/// Largest forward counter step accepted between two consecutive *observed* reports.
///
/// The device emits ~1-4 reports/s; a 500 ms read should see 0-2 steps. 32 steps at that cadence is
/// 64 reports/s, over an order of magnitude above the measured peak, while still short of
/// [`RESTART_WINDOW`]. A larger jump is ambiguous, not proof that a write landed.
pub(super) const MAX_FORWARD_STEPS_PER_READ: u32 = 32;

/// What one report says about its place in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReportOrder {
    Initial,
    Fresh,
    Duplicate,
    /// Out of order, implausibly far ahead, or a reset with an older/equal clock.
    Ambiguous,
    /// A counter reset with a *newer* clock: the reboot can be told from a wrap.
    Restart,
}

/// Classify `current` against the last accepted report before changing any controller state.
pub(super) fn classify(previous: Option<&Report>, current: &Report) -> ReportOrder {
    let Some(previous) = previous else {
        return ReportOrder::Initial;
    };
    let delta = current.message_id.wrapping_sub(previous.message_id) & 0xFF;
    if delta == 0 {
        return ReportOrder::Duplicate;
    }
    let clock_newer = current.timestamp > previous.timestamp;
    let clock_older = current.timestamp > 0 && current.timestamp < previous.timestamp;
    if delta > RESTART_WINDOW {
        // A wrap is a small forward delta (250 -> 0 is 6), so a large backward ring distance is a
        // reset/replay. Only a newer device clock makes that distinguishable from an old report.
        return if clock_newer {
            ReportOrder::Restart
        } else {
            ReportOrder::Ambiguous
        };
    }
    if delta > MAX_FORWARD_STEPS_PER_READ || clock_older {
        return ReportOrder::Ambiguous;
    }
    // Equal timestamps are normal: the device reports several times per second with 1 s clock
    // resolution, and the counter, not the clock, orders those reports.
    ReportOrder::Fresh
}

/// `acMode` outside {0, 1, 2} means the write loop wedged the AC path (`127`/`251`/`255` seen in
/// the field). 0 is "not reported" and is not a fault; any other value is.
pub fn wedge(report: &Report) -> Option<ControlFault> {
    match report.properties.ac_mode {
        CHARGING | DISCHARGING | 0 => None,
        ac_mode => Some(ControlFault::Wedged {
            sn: report.sn.clone(),
            ac_mode,
        }),
    }
}

/// A device restart with forward-moving clock evidence. Ambiguous resets stay inconclusive.
pub fn restart(previous: &Report, current: &Report) -> Option<ControlFault> {
    (classify(Some(previous), current) == ReportOrder::Restart).then(|| ControlFault::Restarted {
        sn: current.sn.clone(),
        before: previous.message_id,
        after: current.message_id,
    })
}

/// Wedge first (it forbids all writes), restart second.
pub fn inspect(previous: Option<&Report>, current: &Report) -> Option<ControlFault> {
    wedge(current).or_else(|| previous.and_then(|previous| restart(previous, current)))
}
