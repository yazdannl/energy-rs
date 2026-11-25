//! Turning one send (or its per-pack deadline) into the published dispatch row.

use crate::dispatch::sent::Sent;
use crate::dispatch::{AppliedPowerReason, PackReport, Planned};
use crate::safety::Reason;

use super::{Dispatcher, backoff};

/// Fold the receipt into the row, preserving the honest "unknown watts" reason.
///
/// Structural invariant: `flow_measured` is derived only from a timestamped sample. A receipt
/// without `measured_at` cannot become a measured row, and a measured row can never carry an
/// `applied_w_reason` — the two states are mutually exclusive by construction here.
pub(super) fn finish(dispatcher: &Dispatcher, plan: Planned, mut sent: Sent) -> PackReport {
    backoff::finish(&dispatcher.last_commands, &plan, &mut sent);
    let measured_at = sent.measured_at.filter(|_| sent.flow_measured);
    let flow_measured = measured_at.is_some();
    let applied_w_reason = if flow_measured {
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
        flow_measured,
        measured_at,
        judged_sample_at: sent.judged_sample_at,
        judged_write_at: sent.judged_write_at,
        applied_w_reason,
        reason: if sent.timeout.is_some() {
            Reason::SendTimedOut
        } else if sent.blocked_by_pending {
            Reason::PreviousCommandPending
        } else if sent.confirmation_pending {
            Reason::ConfirmationPending
        } else {
            plan.reason
        },
        intended_reason: plan.intended_reason,
        written: sent.written,
        accepted_intent_w: sent.accepted_intent_w,
        verified: sent.verified,
        fail_safe: sent.fail_safe,
        superseded: sent.superseded,
        error: sent.error,
        note: plan.note,
        capability_skipped: plan.skip.is_some(),
    }
}

/// A pack skipped on device authority: report the fresh measurement, if there is one, instead of
/// pretending the unwritten row has no flow evidence.
pub(super) fn skipped(plan: &Planned) -> Sent {
    let mut sent = Sent::none();
    if let Some(flow_w) = plan.measured_flow_w {
        sent.applied_w = flow_w;
        sent.flow_measured = true;
        sent.measured_at = plan.measured_at;
    }
    sent
}

/// A send that outlived its budget: the command may have landed, so no accepted write is claimed.
/// The controller was already marked uncertain by the sink and the dispatcher's fail-safe hold is
/// the only reconciliation before ordinary writes are allowed again.
pub(super) fn deadline_sent(recovery_started: bool) -> Sent {
    let state = if recovery_started {
        "recovery started, outcome unknown"
    } else {
        "no recovery attempted"
    };
    Sent {
        timeout: Some("dispatch".to_owned()),
        applied_w_reason: Some(AppliedPowerReason::ReadFailed),
        error: Some(format!(
            "send exceeded the end-to-end pack budget; device state uncertain; {state}"
        )),
        ..Sent::none()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::safety::{Reason, SafetyLimits};

    fn plan() -> Planned {
        Planned {
            pack_id: "SN-A".to_owned(),
            ledger_id: None,
            measured_w: 0.0,
            measured_flow_w: None,
            measured_at: None,
            requested_w: 400.0,
            command_w: 400.0,
            reason: Reason::Ok,
            intended_reason: Reason::Ok,
            note: None,
            backoff_cause: None,
            skip: None,
            vector: true,
            send: true,
        }
    }

    fn dispatcher() -> Dispatcher {
        Dispatcher::new(SafetyLimits::default(), true, 10)
    }

    #[test]
    fn a_claimed_measurement_without_a_sample_is_never_a_measured_row() {
        let sent = |measured_at| Sent {
            flow_measured: true,
            measured_at,
            applied_w_reason: Some(AppliedPowerReason::SampleStale),
            ..Sent::none()
        };
        let row = finish(&dispatcher(), plan(), sent(None));
        assert!(!row.flow_measured);
        assert_eq!(row.measured_at, None);
        assert_eq!(row.applied_w_reason, Some(AppliedPowerReason::SampleStale));

        let row = finish(&dispatcher(), plan(), sent(Some(1_800_000_000)));
        assert!(row.flow_measured);
        assert_eq!(row.measured_at, Some(1_800_000_000));
        assert_eq!(
            row.applied_w_reason, None,
            "a measured row can never claim a stale sample"
        );
    }
}
