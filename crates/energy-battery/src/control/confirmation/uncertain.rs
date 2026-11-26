//! A timed-out write stays uncertain until a verified neutralising hold.

use crate::control::controller::DeviceController;
use crate::control::fault::ControlFault;
use crate::transport::DeviceIo;

/// A timed-out write is uncertain: only a verified hold may authorise another ordinary write.
pub fn mark_uncertain_write<T: DeviceIo>(controller: &DeviceController<T>, detail: String) {
    if let Some(mut inner) = controller.lock() {
        let request_id = inner
            .pending
            .as_ref()
            .map_or(0, |pending| pending.request_id);
        inner.note_fault(ControlFault::UncertainWrite {
            sn: controller.sn().to_owned(),
            request_id,
            detail,
        });
    }
}
