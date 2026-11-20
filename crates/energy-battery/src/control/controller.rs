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

/// Maximum time a neutralising hold waits for an unresolved command before preempting it.
///
/// This is the safety-response bound, deliberately separate from the (longer) echo window: a
/// cell-voltage, shutdown or emergency hold may never be blocked indefinitely by a write whose
/// echo is late. The preempted command is cancelled in place and reported as timed out.
pub const SAFETY_HOLD_WINDOW: Duration = Duration::from_secs(2);

/// The control plane for one device.
///
/// Reads are always allowed; writes are refused until the device has been read at least once.
///
/// Ordering is the safety property, and it is enforced per device:
///
/// * a write takes the pacer's single in-flight slot **first**, and only then re-reads the state,
///   re-clamps against the live maximum and transmits;
/// * the clamped command is installed as [`Pending`] before the request leaves, and no second writer
///   may replace it while unresolved — except a hold, which preempts it after [`SAFETY_HOLD_WINDOW`];
/// * a poll (fetch + fold) and a write (validate + install + transmit) never interleave;
/// * only a report newer than the pre-write report can confirm or fault a command, and a report from
///   another serial is ignored as a fault. Unwrapped counters make the comparison wrap-safe;
/// * every pending command has a bounded resolution path: [`poll_confirming`](Self::poll_confirming)
///   expires it at a hard deadline, and a hold preempts it at the safety bound.
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
        self.lock().and_then(|inner| inner.pending.clone())
    }

    pub fn last_fault(&self) -> Option<ControlFault> {
        self.lock().and_then(|inner| inner.last_fault.clone())
    }

    pub fn is_wedged(&self) -> bool {
        self.inner.lock().is_ok_and(|inner| inner.wedged.is_some())
    }

    /// Read the device and fold the report into the controller's state.
    ///
    /// A wedge or a restart surfaces a fault and pushes the pacer out by [`FAULT_BACKOFF`].
    pub async fn poll(&self) -> Result<Observation, BatteryError> {
        let (report, confirmed, fault) = {
            let _transaction = self.transaction.lock().await;
            let report = self.transport.report().await?;
            let (confirmed, fault) = self.record(&report);
            (report, confirmed, fault)
        };
        if fault.is_some() {
            self.pacer.defer(FAULT_BACKOFF).await;
        }
        Ok(Observation {
            report,
            confirmed,
            fault,
        })
    }

    /// Poll until the command this call was created for is resolved or `window` expires.
    ///
    /// Every read is bounded by the same hard deadline, `on_progress` is called while a read is
    /// still in flight, and a report arriving after the deadline is never accepted. Duplicates,
    /// stalled reads and foreign reports cannot leave the command pending past the deadline; a read
    /// error resolves it immediately so a safety hold is never blocked by a broken transport.
    /// The waiter is bound to its [`request_id`](Pending::request_id): it never expires a command
    /// that replaced the one it started with.
    pub async fn poll_confirming(
        &self,
        window: Duration,
        on_progress: impl Fn(),
    ) -> Result<Observation, BatteryError> {
        let deadline = Instant::now() + window;
        let Some(target) = self.pending().map(|pending| pending.request_id) else {
            return match tokio::time::timeout_at(deadline, self.poll()).await {
                Ok(result) => result,
                Err(_) => Err(BatteryError::Invalid {
                    reason: "report read exceeded the confirmation deadline".to_owned(),
                }),
            };
        };
        match tokio::time::timeout_at(
            deadline,
            confirmation::confirm_loop(self, target, &on_progress),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => confirmation::expire(self, target, "confirmation deadline"),
        }
    }

    /// Pace, revalidate, clamp, send and remember one command.
    ///
    /// A hold has safety priority: after [`SAFETY_HOLD_WINDOW`] it cancels an unresolved command
    /// instead of waiting for its echo. Other commands keep the one-write-at-a-time rule.
    pub async fn apply(&self, command: Command) -> Result<Applied, BatteryError> {
        let claim = self.pacer.claim().await;
        if command.is_hold() {
            confirmation::clear_for_hold(self).await;
        }
        let (pending, ack) = {
            let _transaction = self.transaction.lock().await;
            let pending = self.prepare(command)?;
            let request = WriteRequest::new(
                self.transport.sn().to_owned(),
                pending.request_id,
                pending.command.properties(),
            );
            let outcome = self.transport.write(request).await;
            let ack = self.finish(&pending, outcome);
            (pending, ack)
        };
        drop(claim);
        Ok(Applied {
            command: pending.command,
            ack: ack?,
        })
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

    /// Revalidate and reserve the command, with the write slot already held.
    fn prepare(&self, command: Command) -> Result<Pending, BatteryError> {
        let mut inner = self.lock().ok_or_else(|| BatteryError::Invalid {
            reason: format!("device {} state is poisoned", self.transport.sn()),
        })?;
        let sn = self.transport.sn().to_owned();
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
            acked_after: None,
            acked_sequence: None,
            sent_at: Instant::now(),
            emergency,
            inconclusive: None,
        };
        inner.pending = Some(pending.clone());
        pending
    }

    /// Close the write: an ack's counter is unwrapped from the baseline, including across 0.
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
                    inner.pending = None;
                }
                Err(error)
            }
        }
    }

    fn lock(&self) -> Option<std::sync::MutexGuard<'_, Inner>> {
        self.inner.lock().ok()
    }

    /// Fold one report into the device state.
    fn record(&self, report: &Report) -> (Option<Command>, Option<ControlFault>) {
        let Ok(mut inner) = self.inner.lock() else {
            return (None, None);
        };
        inner.record(report, self.transport.sn())
    }
}
