//! The per-device state machine: identity, wedge, restart and echo confirmation.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::command::Command;
use crate::control::echo;
use crate::control::fault::ControlFault;
use crate::control::flow;
use crate::control::flow_verdict::{self, FlowVerdict};
use crate::control::pending::Pending;
use crate::control::watch;
use crate::report::Report;

type Folded = (
    Option<Command>,
    Option<Command>,
    Option<ControlFault>,
    Option<u32>,
);

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
    /// Override for tests; `None` uses the documented flow window.
    pub(super) flow_window: Option<Duration>,
}

impl Inner {
    /// Fold one report: identity, ordering, wedge, restart, echo. Resync never verifies.
    pub(super) fn record(&mut self, report: &Report, sn: &str) -> Folded {
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
    pub(super) fn note_fault(&mut self, fault: ControlFault) {
        self.last_fault = Some(fault.clone());
        self.active_fault = Some(fault);
    }

    /// Only verified recovery clears the active episode; history deliberately stays.
    fn note_recovery(&mut self, command: Command) {
        let recovered = match &self.active_fault {
            Some(ControlFault::NoFlow { .. }) => !command.is_hold(),
            Some(ControlFault::UncertainWrite { .. }) => command.is_hold(),
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

    /// Verify a pending command with a strictly newer report; stale reads keep ownership.
    fn confirm(&mut self, report: &Report, sequence: u64) -> Folded {
        let Some(mut pending) = self.pending.clone() else {
            return (None, None, None, None);
        };
        if !echo::is_fresh(&pending, report, sequence) {
            return (None, None, None, None);
        }
        pending.last_seen_sequence = sequence;
        self.pending = Some(pending.clone());
        let request_id = pending.request_id;
        let evidence = flow::assess(&pending.command, report);
        match echo::verify(&pending.command, report) {
            Ok(()) if pending.command.is_hold() => {
                self.pending = None;
                (Some(pending.command), None, None, Some(request_id))
            }
            Ok(()) => {
                let window = self.flow_window.unwrap_or(flow::FLOW_CONFIRM_WINDOW);
                let verdict =
                    flow_verdict::assess(&mut pending, report, evidence, Instant::now(), window);
                match verdict {
                    FlowVerdict::Waiting => {
                        pending.inconclusive = None;
                        self.pending = Some(pending);
                        return (None, None, None, None);
                    }
                    FlowVerdict::Fault(fault) => {
                        pending.reset_flow();
                        pending.inconclusive = Some(fault);
                        self.pending = Some(pending);
                        return (None, None, None, None);
                    }
                    FlowVerdict::Qualifying => {}
                }
                if let Some(full) = flow_verdict::qualify(&mut pending, evidence.is_flowing()) {
                    self.pending = None;
                    return if full {
                        (Some(pending.command), None, None, Some(request_id))
                    } else {
                        (None, Some(pending.command), None, Some(request_id))
                    };
                }
                self.pending = Some(pending);
                (None, None, None, None)
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

    /// Finish this waiter; a real mismatch outranks the timeout fallback.
    pub(super) fn expire(
        &mut self,
        request_id: u32,
        fallback: ControlFault,
    ) -> Option<ControlFault> {
        let mut pending = self.pending.take()?;
        if pending.request_id != request_id {
            self.pending = Some(pending);
            return None;
        }
        let fault = pending.inconclusive.take().unwrap_or(fallback);
        // A downgraded no-flow verdict must keep ownership and its original deadline; otherwise
        // later ticks can restart the ramp and never resolve a possibly standing charge.
        if Instant::now() < pending.flow_deadline
            && matches!(&fault, ControlFault::NoFlow { sample_at, write_at, .. }
                if *sample_at <= *write_at
                    || sample_at.saturating_sub(*write_at)
                        < i64::try_from(flow::FLOW_CONFIRM_WINDOW.as_secs()).unwrap_or(i64::MAX))
        {
            pending.inconclusive = Some(fault.clone());
            self.pending = Some(pending);
        }
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
