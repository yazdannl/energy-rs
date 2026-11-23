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
            Ok(receipt) => self.receipt_sent(progress, pack_id, receipt, context),
            Err(error) => {
                self.failure(progress, pack_id, error, sink, context.now)
                    .await
            }
        }
    }

    fn receipt_sent(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        receipt: WriteReceipt,
        context: SendContext,
    ) -> Sent {
        if receipt.confirmation_pending {
            return Self::pending_sent();
        }
        if receipt.blocked_by_pending {
            return Self::blocked_sent();
        }
        if receipt.superseded {
            return Sent {
                superseded: Some("safety hold".to_owned()),
                ..Sent::none()
            };
        }
        if receipt.partial {
            return self.curtailed(progress, pack_id, &receipt, context.now);
        }
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

    async fn failure(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        error: SinkError,
        sink: &dyn CommandSink,
        now: i64,
    ) -> Sent {
        match error {
            SinkError::Fenced { .. } => Sent {
                error: Some(error.to_string()),
                ..Sent::none()
            },
            SinkError::ReadUnverified { reason, .. } => Sent {
                written: true,
                applied_w_reason: Some(AppliedPowerReason::ReadFailed),
                error: Some(reason),
                ..Sent::none()
            },
            SinkError::Timeout { phase, .. } => {
                let error = SinkError::Timeout {
                    pack_id: pack_id.to_owned(),
                    phase: phase.clone(),
                };
                let mut sent = self.fail_safe(progress, now, pack_id, &error, sink).await;
                sent.timeout = Some(phase);
                sent
            }
            error => self.fail_safe(progress, now, pack_id, &error, sink).await,
        }
    }

    /// A changed plan was not written; the prior command still owns the device.
    fn blocked_sent() -> Sent {
        Sent {
            blocked_by_pending: true,
            superseded: Some("previous command pending".to_owned()),
            applied_w_reason: Some(AppliedPowerReason::SampleStale),
            error: Some("previous command still pending; new setpoint not written".to_owned()),
            ..Sent::none()
        }
    }

    /// The write landed; watts stay unknown until the next fleet read resolves the echo and flow.
    fn pending_sent() -> Sent {
        Sent {
            written: true,
            confirmation_pending: true,
            applied_w_reason: Some(AppliedPowerReason::SampleStale),
            error: Some("confirmation pending; readback will verify".to_owned()),
            ..Sent::none()
        }
    }

    /// A pack that really flows below its command is left alone briefly, without a hold or fault.
    fn curtailed(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        receipt: &WriteReceipt,
        now: i64,
    ) -> Sent {
        advance(progress, now);
        let cause = if receipt.flow_measured {
            format!(
                "pack is current-limited at {:.0} W; keeping the command",
                receipt.applied_w.abs()
            )
        } else {
            "partial command; flow unknown".to_owned()
        };
        self.faults
            .arm_pause(pack_id, now, &cause, CURTAILMENT_PAUSE_S);
        Sent {
            written: true,
            flow_measured: receipt.flow_measured,
            applied_w: receipt.applied_w,
            applied_w_reason: receipt_reason(receipt, None),
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
            SinkError::ReadUnverified { .. }
                | SinkError::DeviceUnreachable { .. }
                | SinkError::Timeout { .. }
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
                    confirmation_pending: receipt.confirmation_pending,
                    superseded: receipt.superseded.then(|| "safety hold".to_owned()),
                    error: Some(format!("{cause}; fail-safe hold")),
                    ..Sent::none()
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
/// A receipt only exists after a successful read, so its unmeasured watts are stale/unusable —
/// not a read failure. Preserve an earlier read failure only when the hold is also unmeasured.
fn receipt_reason(
    receipt: &WriteReceipt,
    fallback: Option<AppliedPowerReason>,
) -> Option<AppliedPowerReason> {
    if receipt.flow_measured {
        None
    } else {
        fallback.or(Some(AppliedPowerReason::SampleStale))
    }
}
