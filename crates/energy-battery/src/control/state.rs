//! The per-device state machine: identity, wedge, restart and echo confirmation.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::command::Command;
use crate::control::echo;
use crate::control::fault::ControlFault;
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
    pub(super) last_fault: Option<ControlFault>,
    /// Emergency holds sent in the current wedge episode.
    pub(super) emergency_holds: u32,
}

impl Inner {
    /// Fold one report into the state: identity, ordering, wedge, restart, then echo confirmation.
    ///
    /// The third result is the request id this read resolved, so a waiter only accepts a terminal
    /// observation belonging to its own command. A bounded resync moves the accepted baseline but
    /// never verifies or faults the outstanding command.
    pub(super) fn record(
        &mut self,
        report: &Report,
        sn: &str,
    ) -> (Option<Command>, Option<ControlFault>, Option<u32>) {
        if report.sn != sn {
            let fault = ControlFault::ForeignReport {
                expected: sn.to_owned(),
                got: report.sn.clone(),
            };
            self.last_fault = Some(fault.clone());
            return (None, Some(fault), None);
        }
        let previous = self.last_report.clone();
        let mut order = watch::classify(previous.as_ref(), report);
        if order == watch::ReportOrder::Resync
            && self.pending.as_ref().is_some_and(|p| p.resync_credits == 0)
        {
            order = watch::ReportOrder::Ambiguous;
        }
        let Some(sequence) = self.accepted_sequence(previous.as_ref(), report, order) else {
            return (None, None, None);
        };
        if order == watch::ReportOrder::Resync {
            self.resynchronise(report, sequence);
            return (None, None, None);
        }
        self.last_report = Some(report.clone());
        self.last_sequence = sequence;
        if let Some(fault) = watch::wedge(report) {
            let resolved = self.take_pending();
            self.wedged = Some(report.properties.ac_mode);
            self.last_fault = Some(fault.clone());
            return (None, Some(fault), resolved);
        }
        self.wedged = None;
        self.emergency_holds = 0;
        if order == watch::ReportOrder::Restart {
            let resolved = self.take_pending();
            let fault = ControlFault::Restarted {
                sn: report.sn.clone(),
                before: previous.map(|p| p.message_id).unwrap_or_default(),
                after: report.message_id,
            };
            self.last_fault = Some(fault.clone());
            return (None, Some(fault), resolved);
        }
        let (confirmed, fault, resolved) = self.confirm(report, sequence);
        self.last_fault.clone_from(&fault);
        (confirmed, fault, resolved)
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
    ///
    /// A duplicate, stale or foreign report never confirms and never clears the pending command:
    /// it says nothing about the write, so ownership stays with the command in doubt until its
    /// deadline. A fresh report with the old limits is likewise inconclusive.
    fn confirm(
        &mut self,
        report: &Report,
        sequence: u64,
    ) -> (Option<Command>, Option<ControlFault>, Option<u32>) {
        let Some(mut pending) = self.pending.clone() else {
            return (None, None, None);
        };
        if !echo::is_fresh(&pending, report, sequence) {
            return (None, None, None);
        }
        // Consume this counter step even when the limits are still old: a replay of the same
        // report is not a second observation and must never confirm.
        pending.last_seen_sequence = sequence;
        self.pending = Some(pending.clone());
        let request_id = pending.request_id;
        match echo::verify(&pending.command, report) {
            Ok(()) => {
                self.pending = None;
                (Some(pending.command), None, Some(request_id))
            }
            Err(fault) if echo::is_echo_doubt(&fault) => {
                // The command stays owned; only the decision is deferred to the bounded wait.
                pending.inconclusive = Some(fault);
                self.pending = Some(pending);
                (None, None, None)
            }
            Err(fault) => {
                self.pending = None;
                (None, Some(fault), Some(request_id))
            }
        }
    }

    /// Finish exactly the command this waiter was created for, and no other one.
    ///
    /// A real limit mismatch seen during the wait is the more useful fault; the timeout variant is
    /// only the fallback for duplicates, stalled reports and read errors.
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
        self.last_fault = Some(fault.clone());
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

    /// Atomically cancel an expired pending command for a safety hold.
    ///
    /// `None` means there is nothing left to clear (the slot is genuinely empty). `Some(delay)` keeps
    /// the hold waiting; both the read and the cancellation happen under one lock, so a racing
    /// replacement cannot slip between them.
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
