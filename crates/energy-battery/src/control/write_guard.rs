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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::control::{Command, ControlLimits, Pending};
    use crate::error::BatteryError;
    use crate::report::Report;
    use crate::transport::{BoxFuture, WriteAck};
    use tokio::time::Instant;

    struct Noop;

    impl DeviceIo for Noop {
        fn sn(&self) -> &'static str {
            "TEST"
        }

        fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
            Box::pin(async {
                Err(BatteryError::Invalid {
                    reason: "unused".into(),
                })
            })
        }

        fn write(
            &self,
            _: crate::transport::WriteRequest,
        ) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
            Box::pin(async {
                Err(BatteryError::Invalid {
                    reason: "unused".into(),
                })
            })
        }
    }

    fn pending(request_id: u32) -> Pending {
        Pending {
            command: Command::Hold,
            request_id,
            baseline: (1, 1),
            baseline_sequence: 1,
            last_seen_sequence: 1,
            resync_credits: 1,
            acked_after: None,
            acked_sequence: None,
            sent_at: Instant::now(),
            emergency: false,
            inconclusive: None,
            flow_samples: 0,
            full_flow_samples: 0,
            flow_w: 0,
        }
    }

    #[tokio::test]
    async fn cancelling_one_request_does_not_clear_a_replacement() {
        let controller = DeviceController::new(Noop, ControlLimits::unlimited());
        let guard = PendingGuard::new(&controller, 1);
        controller.lock().expect("state").pending = Some(pending(1));
        controller.lock().expect("state").pending = Some(pending(2));

        drop(guard);

        assert_eq!(controller.pending().map(|entry| entry.request_id), Some(2));
    }
}
