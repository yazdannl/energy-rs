//! The per-device state machine: identity, wedge, restart and echo confirmation.

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
    /// Fold one report into the state: identity, wedge, restart, then echo confirmation.
    ///
    /// A report from another serial is not evidence about this device at all: it becomes a fault and
    /// changes nothing else. A wedge clears the pending command (nothing can be confirmed while the
    /// AC path is out) and keeps the emergency-hold budget until a usable report ends the episode.
    pub(super) fn record(
        &mut self,
        report: &Report,
        sn: &str,
    ) -> (Option<Command>, Option<ControlFault>) {
        if report.sn != sn {
            let fault = ControlFault::ForeignReport {
                expected: sn.to_owned(),
                got: report.sn.clone(),
            };
            self.last_fault = Some(fault.clone());
            return (None, Some(fault));
        }
        let previous = self.last_report.replace(report.clone());
        // Unwrap the 8-bit counter once per observed report. Several complete wraps during a
        // confirmation are then just a larger sequence, not a restart or an old-report replay.
        let sequence = match previous.as_ref() {
            Some(previous) => self.last_sequence.saturating_add(u64::from(echo::forward(
                previous.message_id,
                report.message_id,
            ))),
            None => u64::from(report.message_id),
        };
        self.last_sequence = sequence;
        if let Some(fault) = watch::wedge(report) {
            self.wedged = Some(report.properties.ac_mode);
            self.pending = None;
            self.last_fault = Some(fault.clone());
            return (None, Some(fault));
        }
        // A usable direction ends the wedge episode and refills the emergency-hold budget.
        self.wedged = None;
        self.emergency_holds = 0;
        if let Some(fault) = previous
            .as_ref()
            .and_then(|previous| watch::restart(previous, report))
        {
            // A reboot reverted the device to its flash state; our in-flight write is gone.
            self.pending = None;
            self.last_fault = Some(fault.clone());
            return (None, Some(fault));
        }
        let (confirmed, fault) = self.confirm(report, sequence);
        self.last_fault.clone_from(&fault);
        (confirmed, fault)
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
    ) -> (Option<Command>, Option<ControlFault>) {
        let Some(mut pending) = self.pending.clone() else {
            return (None, None);
        };
        if !echo::is_fresh(&pending, report, sequence) {
            return (None, None);
        }
        // Consume this counter step even when the limits are still old: a replay of the same
        // report is not a second observation and must never confirm.
        pending.last_seen_sequence = sequence;
        self.pending = Some(pending.clone());
        match echo::verify(&pending.command, report) {
            Ok(()) => {
                self.pending = None;
                (Some(pending.command), None)
            }
            Err(fault) if echo::is_echo_doubt(&fault) => {
                // The command stays owned; only the decision is deferred to the bounded wait.
                pending.inconclusive = Some(fault);
                self.pending = Some(pending);
                (None, None)
            }
            Err(fault) => {
                self.pending = None;
                (None, Some(fault))
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

    /// A neutralising hold preempts a younger pending command; it may not cancel someone else's.
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
}
