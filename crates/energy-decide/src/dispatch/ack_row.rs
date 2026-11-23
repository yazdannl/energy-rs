//! Convert a bounded transport result into an honest, explicit row.

use std::sync::Arc;

use crate::dispatch::constants::RAMP_MAX_MS;
use crate::dispatch::fallback::{fallback_accepted, write_fallback};
use crate::dispatch::{
    AppliedReason, AttemptEffect, DispatchOutcome, DispatchRow, EvidenceKind, FallbackResult,
    PackCommand, PackIo, PendingConfirmation, RetryClock, WriteAck, WriteAttempt,
};

fn written_row(command: &PackCommand, ack: &WriteAck, elapsed: u64, budget_ms: u64) -> DispatchRow {
    let mut row = DispatchRow::base(command, budget_ms);
    row.elapsed_ms = elapsed;
    row.written = true;
    row.written_w = Some(command.requested_w);
    row = row.echo(ack.echo(), command.requested_w);
    row
}

fn accepted_row(
    command: &PackCommand,
    ack: &WriteAck,
    elapsed: u64,
    budget_ms: u64,
    now_ms: i64,
) -> AttemptEffect {
    let row = written_row(command, ack, elapsed, budget_ms);
    match ack.echo() {
        Some(echo) if echo.matches(command.requested_w) => {
            let pending = PendingConfirmation::new(
                command.requested_w,
                now_ms,
                Some(echo.at_ms),
                RAMP_MAX_MS,
            );
            AttemptEffect {
                row: row
                    .outcome(
                        DispatchOutcome::ConfirmationPending,
                        EvidenceKind::NoMeasurement,
                    )
                    .reason("flow_not_measured"),
                pending: Some(pending),
                uncertain_reason: None,
                arm_backoff: false,
            }
        }
        Some(_) => AttemptEffect {
            row: row
                .outcome(DispatchOutcome::Rejected, EvidenceKind::PortDecision)
                .reason("echo_mismatch"),
            pending: None,
            uncertain_reason: None,
            arm_backoff: false,
        },
        None => {
            let mut row = row
                .outcome(DispatchOutcome::Uncertain, EvidenceKind::NoMeasurement)
                .reason("echo_missing");
            row.applied_reason = AppliedReason::EchoMissing;
            AttemptEffect {
                row,
                pending: None,
                uncertain_reason: Some("echo_missing"),
                arm_backoff: false,
            }
        }
    }
}

fn failed_row(
    command: &PackCommand,
    reason: &'static str,
    fallback: Option<&FallbackResult>,
    elapsed: u64,
    budget_ms: u64,
) -> AttemptEffect {
    let mut row = DispatchRow::base(command, budget_ms);
    row.elapsed_ms = elapsed;
    row = row.reason(reason);
    row.applied_reason = AppliedReason::TransportFailed;
    row.controller_uncertain = true;
    if let Some(result) = fallback {
        row.retry_offsets_ms.clone_from(&result.offsets_ms);
        if fallback_accepted(result) {
            row = row.reason("fallback_hold_written");
        } else {
            row = row.reason("safety_hold_unverified");
        }
        row.elapsed_ms = elapsed.max(result.elapsed_ms);
    }
    AttemptEffect {
        row: row
            .outcome(DispatchOutcome::Uncertain, EvidenceKind::NoMeasurement)
            .reason("controller_uncertain"),
        pending: None,
        uncertain_reason: Some(reason),
        arm_backoff: false,
    }
}

fn timeout_row(command: &PackCommand, elapsed: u64, budget_ms: u64) -> AttemptEffect {
    let mut row = DispatchRow::base(command, budget_ms)
        .outcome(DispatchOutcome::Uncertain, EvidenceKind::NoMeasurement)
        .reason("write_deadline_mid_transport")
        .reason("controller_uncertain")
        .reason("fallback_budget_exhausted");
    row.elapsed_ms = elapsed;
    row.applied_reason = AppliedReason::TransportDeadline;
    row.controller_uncertain = true;
    AttemptEffect {
        row,
        pending: None,
        uncertain_reason: Some("write_deadline_mid_transport"),
        arm_backoff: false,
    }
}

fn rejected_row(
    command: &PackCommand,
    reason: &'static str,
    elapsed: u64,
    budget_ms: u64,
) -> AttemptEffect {
    let mut row = DispatchRow::base(command, budget_ms)
        .outcome(DispatchOutcome::Rejected, EvidenceKind::PortDecision)
        .reason(reason);
    row.elapsed_ms = elapsed;
    row.applied_reason = AppliedReason::Rejected;
    AttemptEffect {
        row,
        pending: None,
        uncertain_reason: None,
        arm_backoff: false,
    }
}

/// Build a row, pending state and uncertainty effect for one transport attempt.
pub fn from_attempt(
    io: &Arc<dyn PackIo>,
    clock: &dyn RetryClock,
    attempt: WriteAttempt,
    command: &PackCommand,
    budget_ms: u64,
    now_ms: i64,
) -> AttemptEffect {
    let elapsed = attempt.elapsed_ms();
    match attempt {
        WriteAttempt::Ack {
            ack: ack @ WriteAck::Accepted { .. },
            ..
        } => accepted_row(command, &ack, elapsed, budget_ms, now_ms),
        WriteAttempt::Ack {
            ack: WriteAck::Rejected { reason, .. },
            ..
        } => rejected_row(command, reason, elapsed, budget_ms),
        WriteAttempt::Ack {
            ack: WriteAck::Failed { reason, .. },
            ..
        } => {
            let left = budget_ms.saturating_sub(elapsed);
            let fallback = write_fallback(io, clock, command, left);
            failed_row(command, reason, Some(&fallback), elapsed, budget_ms)
        }
        WriteAttempt::Timeout { .. } => timeout_row(command, elapsed, budget_ms),
        WriteAttempt::Failed { reason, .. } => {
            failed_row(command, reason, None, elapsed, budget_ms)
        }
    }
}

/// Fields an accepted transport adds to a row.
pub fn mark_written(row: &mut DispatchRow, command: &PackCommand) {
    row.written = true;
    row.written_w = Some(command.requested_w);
}
