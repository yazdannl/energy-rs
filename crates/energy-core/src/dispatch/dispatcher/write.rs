//! The write side of the tick: one command, one bounded fail-safe hold.

use crate::dispatch::sent::Sent;
use crate::dispatch::{CommandSink, HOLD_DEADBAND_W, Progress, SinkError, advance};

use super::{Dispatcher, SendContext};

impl Dispatcher {
    /// Write one pack, with the one-shot fail-safe hold when it fails.
    pub(super) async fn send(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        watts: f64,
        sink: &dyn CommandSink,
        context: SendContext,
    ) -> Sent {
        // The heartbeat goes into the sink so the write/confirmation wait can mark every
        // successful transport step, not just the tick boundaries around it.
        let result = if watts.abs() <= HOLD_DEADBAND_W {
            sink.hold_with_progress(pack_id, progress, context.now)
                .await
        } else {
            sink.write_with_progress(pack_id, watts, progress, context.now)
                .await
        };
        match result {
            // A safety hold replaced this write; that hold is the episode's one fail-safe action.
            Ok(receipt) if receipt.superseded => Sent {
                superseded: Some("safety hold".to_owned()),
                ..Sent::none()
            },
            Ok(receipt) => {
                // Progress means work happened: a failed write is the watchdog's business, not a
                // heartbeat.
                advance(progress, context.now);
                if receipt.verified && !context.safety_traffic {
                    self.faults.clear(pack_id);
                }
                Sent {
                    written: true,
                    verified: receipt.verified,
                    applied_w: receipt.applied_w,
                    ..Sent::none()
                }
            }
            // The gate withdrew the command (mode change, emergency hold) before it was written.
            // Nothing reached the device, so there is no fault to back off from either.
            Err(error @ SinkError::Fenced { .. }) => Sent {
                error: Some(error.to_string()),
                ..Sent::none()
            },
            // The write was accepted; only the readback missed. Keep ownership, hold nothing and
            // leave the backoff alone: this is not a device fault.
            Err(SinkError::ReadUnverified { reason, .. }) => Sent {
                written: true,
                error: Some(reason),
                ..Sent::none()
            },
            Err(error) => {
                self.fail_safe(progress, context.now, pack_id, &error, sink)
                    .await
            }
        }
    }

    /// One real hold after a fault, then back off (survey §7 trap 10).
    async fn fail_safe(
        &self,
        progress: Option<&dyn Progress>,
        now: i64,
        pack_id: &str,
        error: &SinkError,
        sink: &dyn CommandSink,
    ) -> Sent {
        let cause = error.to_string();
        self.faults.arm(pack_id, now, &cause);
        if !self.failsafe_on_error {
            return Sent {
                error: Some(cause),
                ..Sent::none()
            };
        }
        match sink.hold_with_progress(pack_id, progress, now).await {
            // The hold was sent but its own readback missed. It is already the one hold this fault
            // gets: do not loop into another hold or extend the backoff.
            Err(SinkError::ReadUnverified { reason, .. }) => Sent {
                written: true,
                verified: false,
                fail_safe: true,
                error: Some(format!("{cause}; fail-safe hold not echoed: {reason}")),
                ..Sent::none()
            },
            Ok(receipt) => {
                advance(progress, now);
                Sent {
                    written: true,
                    verified: receipt.verified,
                    applied_w: receipt.applied_w,
                    fail_safe: true,
                    superseded: receipt.superseded.then(|| "safety hold".to_owned()),
                    error: Some(format!("{cause}; fail-safe hold")),
                }
            }
            Err(hold_error) => Sent {
                error: Some(format!("{cause}; fail-safe hold failed: {hold_error}")),
                ..Sent::none()
            },
        }
    }
}
