//! How one post-write report bears on the command's flow evidence.

use std::time::Duration;
use tokio::time::Instant;

use crate::control::fault::ControlFault;
use crate::control::flow::{self, FlowEvidence};
use crate::control::pending::Pending;
use crate::report::Report;

pub(super) enum FlowVerdict {
    /// Real flow in the commanded direction: eligible for the confirmation streak.
    Qualifying,
    /// Absent flow before the command's original window ended: pending, never a pass or fault.
    Waiting,
    /// A failed start after the command's original window ended.
    Fault(ControlFault),
}

/// Record one qualifying report. `Some(true)` means two full-flow reports confirmed.
pub(super) fn qualify(pending: &mut Pending, full: bool) -> Option<bool> {
    pending.inconclusive = None;
    pending.flow_samples = pending.flow_samples.saturating_add(1);
    pending.full_flow_samples = if full {
        pending.full_flow_samples.saturating_add(1)
    } else {
        0
    };
    (pending.flow_samples >= flow::FLOW_SAMPLES)
        .then_some(pending.full_flow_samples >= flow::FLOW_SAMPLES)
}

/// Fold one report into the flow evidence for a pending command.
pub(super) fn assess(
    pending: &mut Pending,
    report: &Report,
    evidence: FlowEvidence,
    now: Instant,
    _window: Duration,
) -> FlowVerdict {
    let measured = flow::measured_w(&pending.command, report);
    pending.flow_w = pending.flow_w.max(measured);
    if !evidence.is_absent() {
        return FlowVerdict::Qualifying;
    }
    // An absent sample inside the ramp is expected and carries no verdict. The deadline was fixed
    // when the command was installed, so later reports cannot restart the observation window.
    if now < pending.flow_deadline {
        pending.reset_flow();
        return FlowVerdict::Waiting;
    }
    let direction = pending
        .command
        .direction()
        .unwrap_or(crate::control::direction::Direction::Charge);
    FlowVerdict::Fault(ControlFault::NoFlow {
        sn: report.sn.clone(),
        direction,
        echoed_w: direction.limit(&report.properties),
        observed_w: measured,
        sample_at: report.timestamp,
        write_at: pending.accepted_at,
    })
}
