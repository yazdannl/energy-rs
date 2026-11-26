//! Public read/acknowledge surface for the controller's fault history.

use crate::control::controller::DeviceController;
use crate::control::fault::ControlFault;
use crate::transport::DeviceIo;

impl<T: DeviceIo> DeviceController<T> {
    /// Test-only flow window; the verdict still needs a strictly post-write report.
    #[must_use]
    pub fn with_flow_window(self, window: std::time::Duration) -> Self {
        if let Some(mut inner) = self.lock() {
            inner.flow_window = Some(window);
        }
        self
    }

    /// The historical fault, kept until verified recovery or explicit acknowledgement.
    pub fn last_fault(&self) -> Option<ControlFault> {
        self.lock().and_then(|inner| inner.last_fault.clone())
    }

    /// The fault episode still active on the device, separate from history and backoff.
    pub fn active_fault(&self) -> Option<ControlFault> {
        self.lock().and_then(|inner| inner.active_fault.clone())
    }

    /// Acknowledge the fault history, e.g. after a physical intervention.
    ///
    /// This never clears an unresolved active episode: health must keep warning until a verified
    /// hold (or an explicit reconcile with a recorded reason) proves the device state is known.
    pub fn acknowledge_fault(&self) {
        if let Some(mut inner) = self.lock() {
            inner.last_fault = None;
        }
    }
}
