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
        let (confirmed, fault) = self.confirm(report);
        self.last_fault.clone_from(&fault);
        (confirmed, fault)
    }

    /// Verify the pending command against a report that is actually newer than the command.
    ///
    /// A duplicate, stale or foreign report never confirms and never clears the pending command: it
    /// says nothing about the write, so ownership stays with the command that is in doubt.
    fn confirm(&mut self, report: &Report) -> (Option<Command>, Option<ControlFault>) {
        let Some(pending) = self.pending else {
            return (None, None);
        };
        if !echo::is_fresh(&pending, report) {
            return (None, None);
        }
        self.pending = None;
        match echo::verify(&pending.command, report) {
            Ok(()) => (Some(pending.command), None),
            Err(fault) => (None, Some(fault)),
        }
    }
}
