//! The write side of the tick: one command, one bounded fail-safe hold.

use energy_battery::ControlFault;

use crate::dispatch::sent::Sent;
use crate::dispatch::{
    AppliedPowerReason, CURTAILMENT_PAUSE_S, CommandSink, HOLD_DEADBAND_W, Progress, SinkError,
    WriteReceipt, advance,
};

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
            // Real but low flow is a completed episode, not a fault: keep ownership, make no hold,
            // and pause ordinary writes so a current-limited pack is not re-commanded every tick.
            Ok(receipt) if receipt.partial => Sent {
                flow_measured: true,
                ..self.curtailed(progress, pack_id, receipt.applied_w, context.now)
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
                    flow_measured: receipt.flow_measured,
                    applied_w: receipt.applied_w,
                    applied_w_reason: receipt_reason(&receipt, None),
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
                applied_w_reason: Some(AppliedPowerReason::ReadFailed),
                error: Some(reason),
                ..Sent::none()
            },
            Err(error) => {
                self.fail_safe(progress, context.now, pack_id, &error, sink)
                    .await
            }
        }
    }

    /// A pack that really flows below its command is left alone briefly, without a hold or fault.
    fn curtailed(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        applied_w: f64,
        now: i64,
    ) -> Sent {
        advance(progress, now);
        let cause = format!(
            "pack is current-limited at {:.0} W; keeping the command",
            applied_w.abs()
        );
        self.faults
            .arm_pause(pack_id, now, &cause, CURTAILMENT_PAUSE_S);
        Sent {
            written: true,
            applied_w,
            ..Sent::none()
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
        // A restart or no-flow gets minutes of silence; a restart gets no hold at all.
        let fault = match error {
            SinkError::Fault { fault, .. } => Some(&**fault),
            _ => None,
        };
        let restarted = matches!(fault, Some(ControlFault::Restarted { .. }));
        let no_flow = matches!(fault, Some(ControlFault::NoFlow { .. }));
        if restarted || no_flow {
            self.faults.arm_long(pack_id, now, &cause);
        } else {
            self.faults.arm(pack_id, now, &cause);
        }
        let read_failed = matches!(
            error,
            SinkError::ReadUnverified { .. } | SinkError::DeviceUnreachable { .. }
        );
        let applied_w_reason = read_failed.then_some(AppliedPowerReason::ReadFailed);
        if !self.failsafe_on_error || restarted {
            return Sent {
                applied_w_reason,
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
                applied_w_reason: Some(AppliedPowerReason::ReadFailed),
                error: Some(format!("{cause}; fail-safe hold not echoed: {reason}")),
                ..Sent::none()
            },
            Ok(receipt) => {
                advance(progress, now);
                Sent {
                    written: true,
                    verified: receipt.verified,
                    flow_measured: receipt.flow_measured,
                    applied_w: receipt.applied_w,
                    applied_w_reason: receipt_reason(&receipt, applied_w_reason),
                    fail_safe: true,
                    superseded: receipt.superseded.then(|| "safety hold".to_owned()),
                    error: Some(format!("{cause}; fail-safe hold")),
                }
            }
            Err(hold_error) => Sent {
                applied_w_reason: Some(AppliedPowerReason::ReadFailed),
                error: Some(format!("{cause}; fail-safe hold failed: {hold_error}")),
                ..Sent::none()
            },
        }
    }
}

/// Why a receipt carries no measured watts: a fresh-sample failure unless a read already failed.
fn receipt_reason(
    receipt: &WriteReceipt,
    fallback: Option<AppliedPowerReason>,
) -> Option<AppliedPowerReason> {
    fallback.or(if receipt.flow_measured {
        None
    } else if receipt.verified {
        Some(AppliedPowerReason::SampleStale)
    } else {
        Some(AppliedPowerReason::ReadFailed)
    })
}
