//! One device: pacing, clamping, echo confirmation and fault memory.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::Mutex as Transaction;

use crate::control::applied::Applied;
use crate::control::clamp::clamp_command;
use crate::control::command::Command;
use crate::control::fault::ControlFault;
use crate::control::limits::ControlLimits;
use crate::control::observation::Observation;
use crate::control::pacer::Pacer;
use crate::control::pending::Pending;
use crate::control::state::Inner;
use crate::error::BatteryError;
use crate::report::Report;
use crate::transport::{DeviceIo, WriteAck, WriteRequest};

/// How long a restarted or wedged device is left alone before the next write attempt.
pub const FAULT_BACKOFF: Duration = Duration::from_secs(10);

/// Emergency neutralising holds a wedged device gets per wedge episode before it is left alone.
///
/// A hold writes no `acMode`, so it is the one write the protocol still allows on a wedged AC path:
/// it takes the pack off the AC side instead of leaving it at the last setpoint. The budget keeps a
/// firmware that ignores holds from turning the failsafe into a write loop.
pub const MAX_EMERGENCY_HOLDS: u32 = 3;

/// The control plane for one device.
///
/// Reads are always allowed; writes are refused until the device has been read at least once.
///
/// Ordering is the safety property, and it is enforced per device:
///
/// * a write takes the pacer's single in-flight slot **first**, and only then re-reads the state,
///   re-clamps against the live maximum and transmits. Waiting for the slot can take seconds, during
///   which a poll can discover a lower limit, a restart or a wedge, so nothing decided before the
///   slot is trusted at transmission time;
/// * the clamped command is installed as [`Pending`] *before* the request leaves, so a report that
///   arrives while the write is in flight is verified rather than consumed unheard, and no second
///   writer may replace a command that is still unresolved;
/// * a poll (fetch + fold) and a write (validate + install + transmit) never interleave, so the
///   readback of one command belongs to that command;
/// * only a report newer than the pre-write report can confirm or fault the pending command, and a
///   report from another serial is ignored as a fault;
/// * a wedged device rejects every command except a bounded emergency hold.
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

    /// Override the write spacing (tests; production keeps the 2 s floor).
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
        self.lock().and_then(|inner| inner.pending)
    }

    pub fn last_fault(&self) -> Option<ControlFault> {
        self.lock().and_then(|inner| inner.last_fault.clone())
    }

    pub fn is_wedged(&self) -> bool {
        self.inner.lock().is_ok_and(|inner| inner.wedged.is_some())
    }

    /// Read the device and fold the report into the controller's state.
    ///
    /// A wedge or a restart means "back off, re-read, surface a fault": the fault is returned, and
    /// the pacer is pushed out by [`FAULT_BACKOFF`] so a caller that ignores the fault still cannot
    /// write for a while.
    pub async fn poll(&self) -> Result<Observation, BatteryError> {
        let (report, confirmed, fault) = {
            let _transaction = self.transaction.lock().await;
            let report = self.transport.report().await?;
            let (confirmed, fault) = self.record(&report);
            (report, confirmed, fault)
        };
        if fault.is_some() {
            // After the transaction: a claim waiting for the slot may hold it, and only the pacer
            // state is needed here.
            self.pacer.defer(FAULT_BACKOFF).await;
        }
        Ok(Observation {
            report,
            confirmed,
            fault,
        })
    }

    /// Pace, revalidate, clamp, send and remember one command.
    ///
    /// The write slot is taken before the device state is read again, the pending command is
    /// installed before the request is transmitted, and a second command is refused while the
    /// previous one is unresolved (poll a fresh report first).
    pub async fn apply(&self, command: Command) -> Result<Applied, BatteryError> {
        let claim = self.pacer.claim().await;
        let (pending, ack) = {
            let _transaction = self.transaction.lock().await;
            let pending = self.prepare(command)?;
            let request = WriteRequest::new(
                self.transport.sn().to_owned(),
                pending.request_id,
                pending.command.properties(),
            );
            let outcome = self.transport.write(request).await;
            (pending, self.finish(pending, outcome))
        };
        drop(claim);
        Ok(Applied {
            command: pending.command,
            ack: ack?,
        })
    }

    /// Revalidate and reserve the command, with the write slot already held.
    fn prepare(&self, command: Command) -> Result<Pending, BatteryError> {
        let mut inner = self.lock().ok_or_else(|| self.poisoned())?;
        let sn = self.transport.sn().to_owned();
        if inner.pending.is_some() {
            return Err(BatteryError::Unconfirmed { sn });
        }
        let Some(report) = inner.last_report.clone() else {
            return Err(BatteryError::NotRead { sn });
        };
        if let Some(ac_mode) = inner.wedged {
            return self.prepare_wedged(&mut inner, command, ac_mode);
        }
        let clamped = clamp_command(command, &self.limits, Some(&report))?;
        Ok(self.install(&mut inner, clamped, &report, false))
    }

    /// While wedged only a hold may go out, and only a bounded number of times per episode.
    fn prepare_wedged(
        &self,
        inner: &mut Inner,
        command: Command,
        ac_mode: i32,
    ) -> Result<Pending, BatteryError> {
        let sn = self.transport.sn().to_owned();
        if !command.is_hold() || inner.emergency_holds >= MAX_EMERGENCY_HOLDS {
            return Err(ControlFault::Wedged { sn, ac_mode }.into());
        }
        let Some(report) = inner.last_report.clone() else {
            return Err(BatteryError::NotRead { sn });
        };
        inner.emergency_holds += 1;
        tracing::warn!(
            sn,
            ac_mode,
            attempt = inner.emergency_holds,
            max = MAX_EMERGENCY_HOLDS,
            "wedged device: sending an emergency neutralising hold"
        );
        // A hold has no magnitude, so it needs no ceiling; its write body never carries `acMode`.
        Ok(self.install(inner, command, &report, true))
    }

    fn install(
        &self,
        inner: &mut Inner,
        command: Command,
        report: &Report,
        emergency: bool,
    ) -> Pending {
        let pending = Pending {
            command,
            request_id: self.next_request_id(),
            baseline: (report.message_id, report.timestamp),
            acked_after: None,
            emergency,
        };
        inner.pending = Some(pending);
        pending
    }

    /// Close the write: a request id in the body is only useful with a request id in the state.
    ///
    /// An error means no ack, so nothing a report could confirm was accepted; the reservation is
    /// released and the command is not believed sent.
    fn finish(
        &self,
        pending: Pending,
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
                }
                Ok(ack)
            }
            Err(error) => {
                if inner.pending.is_some_and(|sent| ours(&sent)) {
                    inner.pending = None;
                }
                Err(error)
            }
        }
    }

    fn next_request_id(&self) -> u32 {
        self.request_id
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    fn lock(&self) -> Option<std::sync::MutexGuard<'_, Inner>> {
        self.inner.lock().ok()
    }

    fn poisoned(&self) -> BatteryError {
        BatteryError::Invalid {
            reason: format!("device {} state is poisoned", self.transport.sn()),
        }
    }

    /// Fold one report into the device state.
    fn record(&self, report: &Report) -> (Option<Command>, Option<ControlFault>) {
        let Ok(mut inner) = self.inner.lock() else {
            return (None, None);
        };
        inner.record(report, self.transport.sn())
    }
}
