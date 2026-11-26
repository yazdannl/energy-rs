//! The one-shot neutralising hold, reported without ever dropping the accepted intent.

use super::SendContext;
use crate::dispatch::sent::Sent;
use crate::dispatch::{AppliedPowerReason, CommandSink, Progress, SinkError, advance};

use super::write::receipt_reason;

/// The one-shot neutralising hold, reported without ever dropping the accepted intent.
pub(super) async fn hold_recovery(
    progress: Option<&dyn Progress>,
    pack_id: &str,
    sink: &dyn CommandSink,
    context: SendContext<'_>,
    cause: &str,
) -> Sent {
    match sink
        .hold_with_progress(pack_id, progress, context.now)
        .await
    {
        Err(SinkError::ReadUnverified { reason, .. }) => Sent {
            written: true,
            verified: false,
            fail_safe: true,
            applied_w_reason: Some(AppliedPowerReason::ReadFailed),
            accepted_intent_w: context.accepted_intent_w,
            error: Some(format!("{cause}; fail-safe hold not echoed: {reason}")),
            ..Sent::none()
        },
        Ok(receipt) => {
            advance(progress, context.now);
            Sent {
                written: true,
                verified: receipt.verified,
                flow_measured: receipt.flow_measured,
                measured_at: receipt.measured_at,
                applied_w: receipt.applied_w,
                applied_w_reason: receipt_reason(&receipt, None),
                fail_safe: true,
                accepted_intent_w: context.accepted_intent_w,
                confirmation_pending: receipt.confirmation_pending,
                superseded: receipt.superseded.then(|| "safety hold".to_owned()),
                error: Some(format!("{cause}; fail-safe hold")),
                ..Sent::none()
            }
        }
        Err(hold_error) => Sent {
            applied_w_reason: Some(AppliedPowerReason::ReadFailed),
            accepted_intent_w: context.accepted_intent_w,
            error: Some(format!("{cause}; fail-safe hold failed: {hold_error}")),
            ..Sent::none()
        },
    }
}
