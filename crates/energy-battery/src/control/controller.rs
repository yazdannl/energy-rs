//! One device: pacing, clamping, echo confirmation and fault memory.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::control::applied::Applied;
use crate::control::clamp::clamp_command;
use crate::control::command::Command;
use crate::control::echo;
use crate::control::fault::ControlFault;
use crate::control::limits::ControlLimits;
use crate::control::observation::Observation;
use crate::control::pacer::Pacer;
use crate::control::watch;
use crate::error::BatteryError;
use crate::report::Report;
use crate::transport::{DeviceIo, WriteRequest};

#[derive(Debug, Default)]
struct Inner {
    last_report: Option<Report>,
    pending: Option<Command>,
    wedged: Option<i32>,
    last_fault: Option<ControlFault>,
}

/// How long a restarted or wedged device is left alone before the next write attempt.
pub const FAULT_BACKOFF: Duration = Duration::from_secs(10);

/// The control plane for one device.
///
/// Reads are always allowed; writes are refused until the device has been read at least once, and
/// refused outright while it is wedged. A write is clamped to the config ceiling and the live
/// `chargeMaxLimit`/`inverseMaxPower` before it is paced and sent, and it is only "confirmed" by the
/// next report.
pub struct DeviceController<T: DeviceIo> {
    transport: T,
    limits: ControlLimits,
    pacer: Pacer,
    request_id: AtomicU32,
    inner: Mutex<Inner>,
}

impl<T: DeviceIo> DeviceController<T> {
    pub fn new(transport: T, limits: ControlLimits) -> Self {
        Self {
            transport,
            limits,
            pacer: Pacer::default(),
            request_id: AtomicU32::new(0),
            inner: Mutex::new(Inner::default()),
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
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.last_report.clone())
    }

    pub fn pending(&self) -> Option<Command> {
        self.inner.lock().ok().and_then(|inner| inner.pending)
    }

    pub fn last_fault(&self) -> Option<ControlFault> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.last_fault.clone())
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
        let report = self.transport.report().await?;
        let (confirmed, fault) = self.record(&report);
        if fault.is_some() {
            self.pacer.defer(FAULT_BACKOFF).await;
        }
        Ok(Observation {
            report,
            confirmed,
            fault,
        })
    }

    /// Clamp, pace, send and remember one command.
    pub async fn apply(&self, command: Command) -> Result<Applied, BatteryError> {
        let clamped = self.clamp_for_device(command)?;
        let claim = self.pacer.claim().await;
        let request = WriteRequest::new(
            self.transport.sn().to_owned(),
            self.next_request_id(),
            clamped.properties(),
        );
        let outcome = self.transport.write(request).await;
        drop(claim);
        let ack = outcome?;
        if let Ok(mut inner) = self.inner.lock() {
            inner.pending = Some(clamped);
        }
        Ok(Applied {
            command: clamped,
            ack,
        })
    }

    fn clamp_for_device(&self, command: Command) -> Result<Command, BatteryError> {
        let inner = self.inner.lock().map_err(|_| BatteryError::Invalid {
            reason: format!("device {} state is poisoned", self.transport.sn()),
        })?;
        if let Some(ac_mode) = inner.wedged {
            return Err(ControlFault::Wedged {
                sn: self.transport.sn().to_owned(),
                ac_mode,
            }
            .into());
        }
        let Some(report) = inner.last_report.as_ref() else {
            return Err(BatteryError::NotRead {
                sn: self.transport.sn().to_owned(),
            });
        };
        Ok(clamp_command(command, &self.limits, Some(report)))
    }

    fn next_request_id(&self) -> u32 {
        self.request_id
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    fn record(&self, report: &Report) -> (Option<Command>, Option<ControlFault>) {
        let Ok(mut inner) = self.inner.lock() else {
            return (None, None);
        };
        let previous = inner.last_report.replace(report.clone());
        let mut confirmed = None;

        let fault = if let Some(fault) = watch::wedge(report) {
            inner.wedged = Some(report.properties.ac_mode);
            inner.pending = None;
            Some(fault)
        } else {
            inner.wedged = None;
            Self::confirm(&mut inner, previous.as_ref(), report, &mut confirmed)
        };
        inner.last_fault.clone_from(&fault);
        (confirmed, fault)
    }

    fn confirm(
        inner: &mut Inner,
        previous: Option<&Report>,
        report: &Report,
        confirmed: &mut Option<Command>,
    ) -> Option<ControlFault> {
        if let Some(fault) = previous.and_then(|previous| watch::restart(previous, report)) {
            // A reboot reverted the device to its flash state; our in-flight write is gone.
            inner.pending = None;
            return Some(fault);
        }
        let pending = inner.pending.take()?;
        match echo::verify(&pending, report) {
            Ok(()) => {
                *confirmed = Some(pending);
                None
            }
            Err(fault) => Some(fault),
        }
    }
}
