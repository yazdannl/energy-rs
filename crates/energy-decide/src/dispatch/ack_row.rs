//! Convert a bounded transport result into an honest, explicit row.

use std::sync::{Arc, Mutex};

use crate::dispatch::constants::RAMP_MAX_MS;
use crate::dispatch::fallback::{fallback_accepted, write_fallback};
use crate::dispatch::{
    AppliedReason, AttemptEffect, DispatchOutcome, DispatchRow, EvidenceKind, FallbackResult,
    LateWrite, PackCommand, PackIo, PendingConfirmation, RetryClock, WriteAck, WriteAttempt,
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
                generation_bumped: false,
            }
        }
        Some(_) => AttemptEffect {
            row: row
                .outcome(DispatchOutcome::Rejected, EvidenceKind::PortDecision)
                .reason("echo_mismatch"),
            pending: None,
            uncertain_reason: None,
            arm_backoff: false,
            generation_bumped: false,
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
                generation_bumped: false,
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
        generation_bumped: false,
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
        generation_bumped: true,
    }
}

fn superseded_effect(
    command: &PackCommand,
    generation: u64,
    elapsed: u64,
    budget_ms: u64,
) -> AttemptEffect {
    let mut effect = timeout_row(command, elapsed, budget_ms);
    effect.row.evidence_kind = EvidenceKind::LateAfterDeadlineSuperseded;
    effect.row.applied_reason = AppliedReason::LateSuperseded;
    effect.row = effect
        .row
        .reason("late_after_deadline_superseded")
        .reason("ignored_superseded_completion");
    effect.row.generation = generation;
    effect
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
        generation_bumped: false,
    }
}

/// Bounded transport context shared by all attempts in one slot.
pub struct AttemptContext<'a> {
    /// Shared transport.
    pub io: &'a Arc<dyn PackIo>,
    /// Retry clock for fail-safe offsets.
    pub clock: &'a dyn RetryClock,
    /// Late completions from superseded generations.
    pub late: &'a Arc<Mutex<Vec<LateWrite>>>,
    /// End-to-end budget for one pack.
    pub budget_ms: u64,
    /// Slot time in epoch milliseconds.
    pub now_ms: i64,
}

/// Build a row, pending state and uncertainty effect for one transport attempt.
pub fn from_attempt(
    context: &AttemptContext<'_>,
    attempt: WriteAttempt,
    command: &PackCommand,
) -> AttemptEffect {
    let elapsed = attempt.elapsed_ms();
    match attempt {
        WriteAttempt::Ack {
            ack: ack @ WriteAck::Accepted { .. },
            ..
        } => accepted_row(command, &ack, elapsed, context.budget_ms, context.now_ms),
        WriteAttempt::Ack {
            ack: WriteAck::Rejected { reason, .. },
            ..
        } => rejected_row(command, reason, elapsed, context.budget_ms),
        WriteAttempt::Ack {
            ack: WriteAck::Failed { reason, .. },
            ..
        } => {
            let left = context.budget_ms.saturating_sub(elapsed);
            let fallback = write_fallback(context.io, context.clock, command, left, context.late);
            failed_row(command, reason, Some(&fallback), elapsed, context.budget_ms)
        }
        WriteAttempt::Timeout { .. } => timeout_row(command, elapsed, context.budget_ms),
        WriteAttempt::Superseded { generation, .. } => {
            superseded_effect(command, generation, elapsed, context.budget_ms)
        }
        WriteAttempt::Failed { reason, .. } => {
            failed_row(command, reason, None, elapsed, context.budget_ms)
        }
    }
}

/// Fields an accepted transport adds to a row.
pub fn mark_written(row: &mut DispatchRow, command: &PackCommand) {
    row.written = true;
    row.written_w = Some(command.requested_w);
}
