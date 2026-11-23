//! Cancellation-safe ownership of an in-flight write's pending entry.

use crate::control::controller::DeviceController;
use crate::transport::DeviceIo;

/// Clears only the command this writer installed if the future is dropped before it resolves.
///
/// `prepare` installs pending state before the write leaves. If the caller cancels `apply` while the
/// transport future is outstanding, `finish` never runs; without this guard the next ordinary command
/// would fail `Unconfirmed` until some unrelated report happened to expire it. Disarm on every normal
/// resolution so a timed-out-but-possibly-accepted write keeps its uncertain ownership deliberately.
pub(super) struct PendingGuard<'a, T: DeviceIo> {
    controller: &'a DeviceController<T>,
    request_id: u32,
    armed: bool,
}

impl<'a, T: DeviceIo> PendingGuard<'a, T> {
    pub(super) fn new(controller: &'a DeviceController<T>, request_id: u32) -> Self {
        Self {
            controller,
            request_id,
            armed: true,
        }
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<T: DeviceIo> Drop for PendingGuard<'_, T> {
    fn drop(&mut self) {
        if self.armed {
            self.controller.cancel(self.request_id);
        }
    }
}
