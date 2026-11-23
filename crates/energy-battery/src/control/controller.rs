//! One device: pacing, clamping, echo confirmation and fault memory.

use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

use tokio::sync::Mutex as Transaction;
use tokio::time::Instant;

use crate::control::applied::Applied;
use crate::control::clamp::clamp_command;
use crate::control::command::Command;
use crate::control::confirmation;
use crate::control::echo;
use crate::control::emergency;
use crate::control::fault::ControlFault;
use crate::control::limits::ControlLimits;
use crate::control::observation::Observation;
use crate::control::pacer::Pacer;
use crate::control::pending::Pending;
use crate::control::state::Inner;
use crate::control::watch::RESTART_WINDOW;
use crate::control::write_bound::{
    POLL_BUSY_TIMEOUT, WRITE_TRANSACTION_TIMEOUT, bounded_report, bounded_write,
};
use crate::control::write_guard::PendingGuard;
use crate::error::BatteryError;
use crate::report::Report;
use crate::transport::{DeviceIo, WriteAck, WriteRequest};

/// Backoff after a restart/wedge; re-writing a rebooting pack caused the 2026-11-21 write storm.
pub const FAULT_BACKOFF: Duration = Duration::from_secs(300);

/// Emergency holds per wedge episode; a hold writes no `acMode`, and this bounds ignored holds.
pub const MAX_EMERGENCY_HOLDS: u32 = 3;

/// Safety preemption bound for an unresolved command; the waiter resolves as superseded.
pub const SAFETY_HOLD_WINDOW: Duration = Duration::from_secs(2);

/// The control plane for one device. Writes need a prior read. Ordering is the safety property:
/// the pacer slot is claimed before validation, pending is installed before the write leaves, polls
/// and writes never interleave, only a newer report confirms, and every command has a bounded exit
/// via [`poll_confirming`](Self::poll_confirming) or a preempting hold.
pub struct DeviceController<T: DeviceIo> {
    transport: T,
    limits: ControlLimits,
    pacer: Pacer,
    request_id: AtomicU32,
    inner: Mutex<Inner>,
    /// One per-device transaction: a write and a readback cannot interleave.
    transaction: Transaction<()>,
}

impl<T: DeviceIo> DeviceController<T> {
    pub fn new(transport: T, limits: ControlLimits) -> Self {
        Self {
            transport,
            limits,
            pacer: Pacer::default(),
            request_id: AtomicU32::new(0),
            inner: Mutex::new(Inner::default()),
            transaction: Transaction::new(()),
        }
    }

    #[must_use]
    pub fn with_spacing(mut self, spacing: Duration) -> Self {
        self.pacer = Pacer::new(spacing);
        self
    }

    pub fn sn(&self) -> &str {
        self.transport.sn()
    }

    pub fn limits(&self) -> ControlLimits {
        self.limits
    }

    pub fn spacing(&self) -> Duration {
        self.pacer.spacing()
    }

    pub fn last_report(&self) -> Option<Report> {
        self.lock().and_then(|inner| inner.last_report.clone())
    }

    pub fn pending(&self) -> Option<Pending> {
        self.lock().and_then(|inner| inner.pending.clone())
    }

    pub fn is_wedged(&self) -> bool {
        self.inner.lock().is_ok_and(|inner| inner.wedged.is_some())
    }

    /// Read the device and fold the report into the controller's state.
    ///
    /// A wedge or a restart surfaces a fault and pushes the pacer out by [`FAULT_BACKOFF`].
    pub async fn poll(&self) -> Result<Observation, BatteryError> {
        let (report, (confirmed, partial, fault, resolved_request)) = {
            let _transaction = tokio::time::timeout(POLL_BUSY_TIMEOUT, self.transaction.lock())
                .await
                .map_err(|_| BatteryError::Invalid {
                    reason: format!("device {} is writing; poll skipped", self.transport.sn()),
                })?;
            let report = bounded_report(self.transport.sn(), self.transport.report()).await?;
            let observed = self.record(&report);
            (report, observed)
        };
        if fault.is_some() {
            self.pacer.defer(FAULT_BACKOFF).await;
        }
        Ok(Observation::observed(
            report,
            confirmed,
            partial,
            fault,
            resolved_request,
        ))
    }

    pub async fn poll_confirming(
        &self,
        window: Duration,
        on_progress: impl Fn(),
    ) -> Result<Observation, BatteryError> {
        confirmation::wait(self, window, &on_progress).await
    }

    pub async fn apply(&self, command: Command) -> Result<Applied, BatteryError> {
        let claim = self.pacer.claim().await;
        if command.is_hold() {
            confirmation::clear_for_hold(self).await;
        }
        let (pending, ack) = {
            let _transaction =
                tokio::time::timeout(WRITE_TRANSACTION_TIMEOUT, self.transaction.lock())
                    .await
                    .map_err(|_| BatteryError::Invalid {
                        reason: format!("device {} is reading; write skipped", self.transport.sn()),
                    })?;
            let pending = self.prepare(command)?;
            let mut guard = PendingGuard::new(self, pending.request_id);
            let request = WriteRequest::new(
                self.transport.sn().to_owned(),
                pending.request_id,
                pending.command.properties(),
            );
            let outcome = bounded_write(
                self.transport.sn(),
                pending.request_id,
                self.transport.write(request),
            )
            .await;
            guard.disarm();
            let ack = self.finish(&pending, outcome);
            (pending, ack)
        };
        drop(claim);
        Ok(Applied {
            command: pending.command,
            ack: ack?,
        })
    }

    pub(super) fn clear_pending_for_hold(&self, deadline: Instant) -> Option<Duration> {
        self.lock()
            .and_then(|mut inner| inner.clear_pending_for_hold(Instant::now(), deadline))
    }

    pub(super) fn cancel(&self, request_id: u32) {
        if let Some(mut inner) = self.lock() {
            inner.cancel(request_id);
        }
    }

    pub(super) fn record_timeout(
        &self,
        request_id: u32,
        fallback: ControlFault,
    ) -> Option<ControlFault> {
        self.lock()
            .and_then(|mut inner| inner.expire(request_id, fallback))
    }

    fn prepare(&self, command: Command) -> Result<Pending, BatteryError> {
        let mut inner = self.lock().ok_or_else(|| BatteryError::Invalid {
            reason: format!("device {} state is poisoned", self.transport.sn()),
        })?;
        let sn = self.transport.sn().to_owned();
        match &inner.active_fault {
            Some(fault @ ControlFault::UncertainWrite { .. }) if !command.is_hold() => {
                return Err(BatteryError::Fault(fault.clone()));
            }
            _ => {}
        }
        if inner.pending.is_some() {
            return Err(BatteryError::Unconfirmed { sn });
        }
        let Some(report) = inner.last_report.clone() else {
            return Err(BatteryError::NotRead { sn });
        };
        if let Some(ac_mode) = inner.wedged {
            return emergency::prepare_wedged(self, &mut inner, command, ac_mode);
        }
        let clamped = clamp_command(command, &self.limits, Some(&report))?;
        Ok(self.install(&mut inner, clamped, &report, false))
    }

    pub(super) fn install(
        &self,
        inner: &mut Inner,
        command: Command,
        report: &Report,
        emergency: bool,
    ) -> Pending {
        let request_id = self
            .request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .wrapping_add(1);
        let pending = Pending {
            command,
            request_id,
            baseline: (report.message_id, report.timestamp),
            baseline_sequence: inner.last_sequence,
            last_seen_sequence: inner.last_sequence,
            resync_credits: 1,
            acked_after: None,
            acked_sequence: None,
            sent_at: Instant::now(),
            emergency,
            inconclusive: None,
            flow_samples: 0,
            full_flow_samples: 0,
            flow_w: 0,
        };
        inner.pending = Some(pending.clone());
        pending
    }

    fn finish(
        &self,
        pending: &Pending,
        outcome: Result<WriteAck, BatteryError>,
    ) -> Result<WriteAck, BatteryError> {
        let Ok(mut inner) = self.inner.lock() else {
            return outcome;
        };
        let ours = |candidate: &Pending| candidate.request_id == pending.request_id;
        match outcome {
            Ok(ack) => {
                if let Some(sent) = inner.pending.as_mut().filter(|sent| ours(sent)) {
                    sent.acked_after = ack.message_id.filter(|acked| *acked != pending.baseline.0);
                    sent.acked_sequence = ack.message_id.and_then(|acked| {
                        let delta = echo::forward(pending.baseline.0, acked);
                        (delta > 0 && delta <= RESTART_WINDOW)
                            .then(|| pending.baseline_sequence.saturating_add(u64::from(delta)))
                    });
                }
                Ok(ack)
            }
            Err(error) => {
                if inner.pending.as_ref().is_some_and(ours) {
                    if let BatteryError::Fault(fault @ ControlFault::UncertainWrite { .. }) = &error
                    {
                        if let Some(sent) = inner.pending.as_mut() {
                            sent.inconclusive = Some(fault.clone());
                        }
                        inner.last_fault = Some(fault.clone());
                        inner.active_fault = Some(fault.clone());
                    } else {
                        inner.pending = None;
                    }
                }
                Err(error)
            }
        }
    }

    pub(super) fn lock(&self) -> Option<std::sync::MutexGuard<'_, Inner>> {
        self.inner.lock().ok()
    }

    /// Fold one report into the device state.
    fn record(
        &self,
        report: &Report,
    ) -> (
        Option<Command>,
        Option<Command>,
        Option<ControlFault>,
        Option<u32>,
    ) {
        let Ok(mut inner) = self.inner.lock() else {
            return (None, None, None, None);
        };
        inner.record(report, self.transport.sn())
    }
}
