//! Turning one send (or its per-pack deadline) into the published dispatch row.

use crate::dispatch::sent::Sent;
use crate::dispatch::{AppliedPowerReason, PackReport, Planned};
use crate::safety::Reason;

use super::{Dispatcher, backoff};

/// Fold the receipt into the row, preserving the honest "unknown watts" reason.
pub(super) fn finish(dispatcher: &Dispatcher, plan: Planned, mut sent: Sent) -> PackReport {
    backoff::finish(&dispatcher.last_commands, &plan, &mut sent);
    let applied_w_reason = if sent.flow_measured {
        None
    } else {
        Some(
            sent.applied_w_reason
                .unwrap_or(AppliedPowerReason::SampleStale),
        )
    };
    PackReport {
        pack_id: plan.pack_id,
        ledger_id: plan.ledger_id,
        requested_w: plan.requested_w,
        intended_w: plan.command_w,
        applied_w: sent.applied_w,
        flow_measured: sent.flow_measured,
        applied_w_reason,
        reason: if sent.confirmation_pending {
            Reason::ConfirmationPending
        } else {
            plan.reason
        },
        intended_reason: plan.intended_reason,
        written: sent.written,
        verified: sent.verified,
        fail_safe: sent.fail_safe,
        superseded: sent.superseded,
        error: sent.error,
        note: plan.note,
    }
}

/// A send that outlived its budget: the command may have landed, but flow is unknown.
pub(super) fn deadline_sent() -> Sent {
    Sent {
        written: true,
        confirmation_pending: true,
        applied_w_reason: Some(AppliedPowerReason::SampleStale),
        error: Some("send exceeded its per-pack deadline; confirmation pending".to_owned()),
        ..Sent::none()
    }
}
