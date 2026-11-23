//! Authority and safety are asked here; M4 records the answers.

use crate::dispatch::{
    AppliedReason, DispatchOutcome, PreparedWrite, SafetyGate, SafetyIntervention,
};
use crate::dispatch::{AuthorityDecision, DispatchRow, DispatchState, EvidenceKind, PackCommand};
use crate::dispatch::{SlotRequest, WriteAuthority};

/// Either a write to start or a row that already decided the pack's fate.
#[derive(Debug, Clone, PartialEq)]
pub enum Preparation {
    /// Send this command under the budget.
    Write(PreparedWrite),
    /// Nothing may be written; report this row.
    Row(DispatchRow),
}

fn denied_row(command: &PackCommand, reason: &'static str, budget_ms: u64) -> DispatchRow {
    DispatchRow::base(command, budget_ms)
        .outcome(DispatchOutcome::Skipped, EvidenceKind::PortDecision)
        .reason("authority_denied")
        .reason(reason)
}

fn deferred_row(command: &PackCommand, reason: &'static str, retry_at: Option<i64>) -> DispatchRow {
    let mut row = DispatchRow::base(command, 0)
        .outcome(DispatchOutcome::Deferred, EvidenceKind::PortDecision)
        .reason(reason);
    row.applied_reason = AppliedReason::Deferred;
    row.retry_at = retry_at;
    row
}

fn safety_hold_command(request: &SlotRequest, target_w: f64, now_ms: i64) -> PackCommand {
    let mut command = PackCommand::from_request(request);
    command.requested_w = target_w;
    command.issued_at_ms = now_ms;
    command.safety_traffic = true;
    command
}

/// Ask M6 then M5, in that order, and never override either answer.
pub fn prepare<S: SafetyGate, A: WriteAuthority>(
    state: &mut DispatchState,
    request: &SlotRequest,
    now_ms: i64,
    safety: &S,
    authority: &A,
    budget_ms: u64,
) -> Preparation {
    let command = PackCommand::from_request(request);
    let pack = state.pack(&request.pack_id);
    let mut interrupted = false;
    // A changed target means a different issued setpoint, so exact equality is intentional.
    #[allow(clippy::float_cmp)]
    if !request.safety_traffic
        && now_ms < pack.pause_until_ms
        && pack
            .pause_target_w
            .is_some_and(|target| target != request.requested_w)
    {
        pack.pause_until_ms = 0;
        pack.partial_retries = 0;
        pack.pause_target_w = None;
        interrupted = true;
    }
    if !request.safety_traffic {
        if pack.uncertain {
            return Preparation::Row(deferred_row(&command, "controller_uncertain", None));
        }
        if now_ms < pack.backoff_until_ms {
            return Preparation::Row(deferred_row(
                &command,
                pack.backoff_reason.unwrap_or("ordinary_backoff"),
                Some(pack.backoff_until_ms),
            ));
        }
        if now_ms < pack.pause_until_ms {
            return Preparation::Row(deferred_row(
                &command,
                "partial_flow_pause",
                Some(pack.pause_until_ms),
            ));
        }
    }
    match authority.authorize(&command) {
        AuthorityDecision::Denied { reason } => {
            Preparation::Row(denied_row(&command, reason, budget_ms))
        }
        AuthorityDecision::Authorized => match safety.preempt(request, now_ms) {
            SafetyIntervention::Proceed => Preparation::Write(PreparedWrite {
                command,
                tx_id: 0,
                gate_reason: None,
                request: request.clone(),
                interrupted,
            }),
            SafetyIntervention::Hold { action, reason } => Preparation::Write(PreparedWrite {
                command: safety_hold_command(request, action.target_w(), now_ms),
                tx_id: 0,
                gate_reason: Some(reason),
                request: request.clone(),
                interrupted,
            }),
        },
    }
}
