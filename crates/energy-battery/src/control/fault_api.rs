//! Public read/acknowledge surface for the controller's fault history.

use crate::control::controller::DeviceController;
use crate::control::fault::ControlFault;
use crate::transport::DeviceIo;

impl<T: DeviceIo> DeviceController<T> {
    /// The historical fault, kept until verified recovery or explicit acknowledgement.
    pub fn last_fault(&self) -> Option<ControlFault> {
        self.lock().and_then(|inner| inner.last_fault.clone())
    }

    /// The fault episode still active on the device, separate from history and backoff.
    pub fn active_fault(&self) -> Option<ControlFault> {
        self.lock().and_then(|inner| inner.active_fault.clone())
    }

    /// Acknowledge the fault history, e.g. after a physical intervention.
    pub fn acknowledge_fault(&self) {
        if let Some(mut inner) = self.lock() {
            inner.active_fault = None;
            inner.last_fault = None;
        }
    }
}
