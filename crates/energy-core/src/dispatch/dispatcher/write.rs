//! The write side of the tick: one command, one bounded fail-safe hold.

use std::sync::atomic::Ordering;

use energy_battery::ControlFault;
use energy_battery::control::{FLOW_EVIDENCE_RECOVERY_WINDOW, flow_evidence_causal};

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
        context: SendContext<'_>,
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
            Err(error) => self.failure(progress, pack_id, error, sink, context).await,
        }
    }

    fn receipt_sent(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        receipt: WriteReceipt,
        context: SendContext<'_>,
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
            measured_at: receipt.measured_at,
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
        context: SendContext<'_>,
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
                let mut sent = self
                    .fail_safe(progress, pack_id, &error, sink, context)
                    .await;
                sent.timeout = Some(phase);
                sent
            }
            error => {
                self.fail_safe(progress, pack_id, &error, sink, context)
                    .await
            }
        }
    }

    /// A changed plan was not written; the prior command still owns the device.
    fn blocked_sent() -> Sent {
        Sent {
            blocked_by_pending: true,
            superseded: Some("previous command pending".to_owned()),
            applied_w_reason: Some(AppliedPowerReason::PreviousCommandPending),
            error: Some("previous command still pending; new setpoint not written".to_owned()),
            ..Sent::none()
        }
    }

    /// The write landed; watts stay unknown until the next fleet read resolves the echo and flow.
    fn pending_sent() -> Sent {
        Sent {
            written: true,
            confirmation_pending: true,
            applied_w_reason: Some(AppliedPowerReason::ConfirmationPending),
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
            measured_at: receipt.measured_at,
            applied_w: receipt.applied_w,
            applied_w_reason: receipt_reason(receipt, None),
            ..Sent::none()
        }
    }

    async fn fail_safe(
        &self,
        progress: Option<&dyn Progress>,
        pack_id: &str,
        error: &SinkError,
        sink: &dyn CommandSink,
        context: SendContext<'_>,
    ) -> Sent {
        context.recovery_started.store(true, Ordering::Relaxed);
        let now = context.now;
        let cause = error.to_string();
        let restarted = matches!(error, SinkError::Fault { fault, .. } if matches!(**fault, ControlFault::Restarted { .. }));
        let verdict_clocks = flow_verdict_clocks(error);
        // The write was accepted before recovery began. Its intent survives a failed or
        // unproved neutralisation so the durable latch still owns the nonzero command.
        let accepted_intent = context.accepted_intent_w;
        if let Some(mut sent) = premature_no_flow(verdict_clocks, now) {
            sent.accepted_intent_w = accepted_intent;
            return sent;
        }
        let no_flow = verdict_clocks.0.is_some();
        if restarted || no_flow {
            self.faults.arm_long(pack_id, now, &cause);
        } else {
            self.faults.arm(pack_id, now, &cause);
        }
        let read_failed = is_read_failure(error);
        let applied_w_reason = read_failed.then_some(AppliedPowerReason::ReadFailed);
        if !self.failsafe_on_error || restarted {
            return Sent {
                applied_w_reason,
                accepted_intent_w: accepted_intent,
                error: Some(cause),
                ..Sent::none()
            }
            .with_judged(verdict_clocks);
        }
        super::recovery::hold_recovery(progress, pack_id, sink, context, &cause)
            .await
            .with_judged(verdict_clocks)
    }
}

/// A no-flow verdict without causally valid post-write evidence stays pending until the original
/// recovery window ends; after that the caller escalates to a verified hold instead of resetting.
fn premature_no_flow(clocks: (Option<i64>, Option<i64>), now: i64) -> Option<Sent> {
    let (Some(sample_at), Some(write_at)) = clocks else {
        return None;
    };
    if flow_window_elapsed(sample_at, write_at) {
        return None;
    }
    let bound = i64::try_from(FLOW_EVIDENCE_RECOVERY_WINDOW.as_secs()).unwrap_or(i64::MAX);
    if now.saturating_sub(write_at) >= bound {
        return None;
    }
    Some(Sent {
        written: true,
        confirmation_pending: true,
        applied_w_reason: Some(AppliedPowerReason::ConfirmationPending),
        error: Some("confirmation pending; readback will verify".to_owned()),
        judged_sample_at: Some(sample_at),
        judged_write_at: Some(write_at),
        ..Sent::none()
    })
}

fn flow_window_elapsed(sample_at: i64, write_at: i64) -> bool {
    flow_evidence_causal(sample_at, write_at)
}

fn is_read_failure(error: &SinkError) -> bool {
    matches!(
        error,
        SinkError::ReadUnverified { .. }
            | SinkError::DeviceUnreachable { .. }
            | SinkError::Timeout { .. }
    )
}

/// Clocks behind a no-flow verdict, carried to the dispatch row for diagnosis.
fn flow_verdict_clocks(error: &SinkError) -> (Option<i64>, Option<i64>) {
    match error {
        SinkError::Fault { fault, .. } => match **fault {
            ControlFault::NoFlow {
                sample_at,
                write_at,
                ..
            } => (Some(sample_at), Some(write_at)),
            _ => (None, None),
        },
        _ => (None, None),
    }
}

/// Why a receipt carries no measured watts; an unmeasured hold keeps an earlier read failure.
pub(super) fn receipt_reason(
    receipt: &WriteReceipt,
    fallback: Option<AppliedPowerReason>,
) -> Option<AppliedPowerReason> {
    if receipt.flow_measured {
        None
    } else {
        fallback.or(Some(AppliedPowerReason::SampleStale))
    }
}
