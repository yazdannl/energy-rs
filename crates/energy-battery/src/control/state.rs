//! The per-device state machine: identity, wedge, restart and echo confirmation.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::command::Command;
use crate::control::echo;
use crate::control::fault::ControlFault;
use crate::control::flow;
use crate::control::pending::Pending;
use crate::control::watch;
use crate::report::Report;

/// Everything one controller remembers between calls.
#[derive(Debug, Default)]
pub(super) struct Inner {
    /// The last report that came from this device (a foreign report never replaces it).
    pub(super) last_report: Option<Report>,
    /// Unwrapped counter of `last_report`; adding forward ring distances survives 255→0 wraps.
    pub(super) last_sequence: u64,
    /// The transmitted command no report has confirmed yet.
    pub(super) pending: Option<Pending>,
    /// The `acMode` that wedged the AC path, until a report shows a usable direction again.
    pub(super) wedged: Option<i32>,
    /// Historical fault: stays visible until verified recovery or explicit acknowledgement.
    pub(super) last_fault: Option<ControlFault>,
    /// The fault episode still considered active by this controller.
    pub(super) active_fault: Option<ControlFault>,
    /// Emergency holds sent in the current wedge episode.
    pub(super) emergency_holds: u32,
}

impl Inner {
    /// Fold one report into the state: identity, ordering, wedge, restart, then echo confirmation.
    /// The last result is the request id resolved by this read; resync never verifies a command.
    pub(super) fn record(
        &mut self,
        report: &Report,
        sn: &str,
    ) -> (
        Option<Command>,
        Option<Command>,
        Option<ControlFault>,
        Option<u32>,
    ) {
        if report.sn != sn {
            let fault = ControlFault::ForeignReport {
                expected: sn.to_owned(),
                got: report.sn.clone(),
            };
            self.note_fault(fault.clone());
            return (None, None, Some(fault), None);
        }
        let previous = self.last_report.clone();
        let mut order = watch::classify(previous.as_ref(), report);
        if order == watch::ReportOrder::Resync
            && self.pending.as_ref().is_some_and(|p| p.resync_credits == 0)
        {
            order = watch::ReportOrder::Ambiguous;
        }
        let Some(sequence) = self.accepted_sequence(previous.as_ref(), report, order) else {
            return (None, None, None, None);
        };
        if order == watch::ReportOrder::Resync {
            self.resynchronise(report, sequence);
            return (None, None, None, None);
        }
        self.last_report = Some(report.clone());
        self.last_sequence = sequence;
        if self.pending.is_none()
            && matches!(self.active_fault, Some(ControlFault::Restarted { .. }))
            && order == watch::ReportOrder::Fresh
        {
            // Only an in-order stream proves a reset is over; no-flow needs a commanded recovery.
            self.active_fault = None;
        }
        if let Some(fault) = watch::wedge(report) {
            let resolved = self.take_pending();
            self.wedged = Some(report.properties.ac_mode);
            self.note_fault(fault.clone());
            return (None, None, Some(fault), resolved);
        }
        if self.wedged.take().is_some() {
            self.active_fault = None;
        }
        self.emergency_holds = 0;
        if order == watch::ReportOrder::Restart {
            let resolved = self.take_pending();
            let fault = ControlFault::Restarted {
                sn: report.sn.clone(),
                before: previous.map(|p| p.message_id).unwrap_or_default(),
                after: report.message_id,
            };
            self.note_fault(fault.clone());
            return (None, None, Some(fault), resolved);
        }
        let (confirmed, partial, fault, resolved) = self.confirm(report, sequence);
        if let Some(fault) = &fault {
            self.note_fault(fault.clone());
        } else if let Some(command) = confirmed.or(partial) {
            self.note_recovery(command);
        }
        (confirmed, partial, fault, resolved)
    }

    /// Record a fault as both the current episode and the durable history.
    fn note_fault(&mut self, fault: ControlFault) {
        self.last_fault = Some(fault.clone());
        self.active_fault = Some(fault);
    }

    /// Only verified recovery clears the active episode; history deliberately stays.
    fn note_recovery(&mut self, command: Command) {
        let recovered = match &self.active_fault {
            Some(ControlFault::NoFlow { .. }) => !command.is_hold(),
            Some(ControlFault::Restarted { .. } | ControlFault::Wedged { .. }) => true,
            _ => false,
        };
        if recovered {
            self.active_fault = None;
        }
    }

    fn accepted_sequence(
        &mut self,
        previous: Option<&Report>,
        report: &Report,
        order: watch::ReportOrder,
    ) -> Option<u64> {
        match order {
            watch::ReportOrder::Initial | watch::ReportOrder::Restart => {
                Some(u64::from(report.message_id))
            }
            watch::ReportOrder::Fresh | watch::ReportOrder::Resync => {
                let delta = previous
                    .map(|previous| echo::forward(previous.message_id, report.message_id))
                    .unwrap_or_default();
                Some(self.last_sequence.saturating_add(u64::from(delta)))
            }
            watch::ReportOrder::Duplicate | watch::ReportOrder::Ambiguous => None,
        }
    }

    /// New baseline for later reports only; the burst itself never verifies a command.
    fn resynchronise(&mut self, report: &Report, sequence: u64) {
        if let Some(pending) = self.pending.as_mut() {
            pending.resync_credits -= 1;
            pending.last_seen_sequence = sequence;
        }
        self.last_report = Some(report.clone());
        self.last_sequence = sequence;
    }

    fn take_pending(&mut self) -> Option<u32> {
        self.pending.take().map(|pending| pending.request_id)
    }

    /// Verify the pending command against a report that is actually newer than the command.
    /// A duplicate, stale or inconclusive report keeps ownership until the bounded deadline.
    fn confirm(
        &mut self,
        report: &Report,
        sequence: u64,
    ) -> (
        Option<Command>,
        Option<Command>,
        Option<ControlFault>,
        Option<u32>,
    ) {
        let Some(mut pending) = self.pending.clone() else {
            return (None, None, None, None);
        };
        if !echo::is_fresh(&pending, report, sequence) {
            return (None, None, None, None);
        }
        // Consume this counter step even when the limits are still old: a replay of the same
        // report is not a second observation and must never confirm.
        pending.last_seen_sequence = sequence;
        self.pending = Some(pending.clone());
        let request_id = pending.request_id;
        // Echo is the first gate; sustained full flow confirms, low flow is partial, zero faults.
        let evidence = flow::assess(&pending.command, report);
        match echo::verify(&pending.command, report) {
            Ok(()) if pending.command.is_hold() => {
                self.pending = None;
                (Some(pending.command), None, None, Some(request_id))
            }
            Ok(()) => {
                if let Some(fault) = Self::note_flow(&mut pending, report, evidence) {
                    // Zero flow (or a broken charge status) is a failed start, not curtailment.
                    pending.reset_flow();
                    pending.inconclusive = Some(fault);
                    self.pending = Some(pending);
                    return (None, None, None, None);
                }
                pending.inconclusive = None;
                pending.flow_samples = pending.flow_samples.saturating_add(1);
                pending.full_flow_samples = if evidence.is_flowing() {
                    pending.full_flow_samples.saturating_add(1)
                } else {
                    0
                };
                if pending.flow_samples < flow::FLOW_SAMPLES {
                    self.pending = Some(pending);
                    return (None, None, None, None);
                }
                self.pending = None;
                if pending.full_flow_samples >= flow::FLOW_SAMPLES {
                    (Some(pending.command), None, None, Some(request_id))
                } else {
                    // Two qualifying but not full reports: real, lower flow, no fault.
                    (None, Some(pending.command), None, Some(request_id))
                }
            }
            Err(fault) if echo::is_echo_doubt(&fault) => {
                // A doubtful echo is a new observation gap: neither streak can span it.
                pending.reset_flow();
                pending.inconclusive = Some(fault);
                self.pending = Some(pending);
                (None, None, None, None)
            }
            Err(fault) => {
                self.pending = None;
                (None, None, Some(fault), Some(request_id))
            }
        }
    }

    /// Fold the flow verdict into the pending command; low-but-real power is only inconclusive.
    fn note_flow(
        pending: &mut Pending,
        report: &Report,
        evidence: flow::FlowEvidence,
    ) -> Option<ControlFault> {
        let measured = flow::measured_w(&pending.command, report);
        pending.flow_w = pending.flow_w.max(measured);
        if !evidence.is_absent() {
            return None;
        }
        let direction = pending
            .command
            .direction()
            .unwrap_or(crate::control::direction::Direction::Charge);
        Some(ControlFault::NoFlow {
            sn: report.sn.clone(),
            direction,
            echoed_w: direction.limit(&report.properties),
            observed_w: measured,
        })
    }

    /// Finish exactly this waiter's command; a real mismatch outranks the timeout fallback.
    pub(super) fn expire(
        &mut self,
        request_id: u32,
        fallback: ControlFault,
    ) -> Option<ControlFault> {
        let pending = self.pending.take()?;
        if pending.request_id != request_id {
            self.pending = Some(pending);
            return None;
        }
        let fault = pending.inconclusive.unwrap_or(fallback);
        self.note_fault(fault.clone());
        Some(fault)
    }

    /// A neutralising hold or a read miss preempts its own pending command, never a replacement.
    pub(super) fn cancel(&mut self, request_id: u32) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            self.pending = None;
            return true;
        }
        false
    }

    /// Atomically cancel an expired pending command for a safety hold; `Some(delay)` means wait.
    pub(super) fn clear_pending_for_hold(
        &mut self,
        now: Instant,
        deadline: Instant,
    ) -> Option<Duration> {
        let pending = self.pending.as_ref()?;
        if now >= deadline || pending.age(now) >= crate::control::controller::SAFETY_HOLD_WINDOW {
            self.pending = None;
            return None;
        }
        Some(deadline.saturating_duration_since(now))
    }
}
