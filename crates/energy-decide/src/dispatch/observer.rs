//! Fold later reports into deferred confirmations; never await them in a tick.

use crate::dispatch::confirmation::{ConfirmationVerdict, fold, idle_confirmed};
use crate::dispatch::constants::{NO_FLOW_BACKOFF_MS, PARTIAL_PAUSE_MS, RAMP_MAX_MS};
use crate::dispatch::{
    AppliedReason, DispatchOutcome, DispatchRow, DispatchState, EvidenceKind, FlowClass,
    FlowReport, PackCommand, PendingConfirmation,
};

fn command_for(pack_id: &str, pending: Option<&PendingConfirmation>) -> PackCommand {
    PackCommand {
        pack_id: pack_id.to_owned(),
        requested_w: pending.map_or(0.0, |v| v.command_w),
        issued_at_ms: pending.map_or(0, |v| v.issued_at_ms),
        generation: 0,
        fence_id: None,
        safety_traffic: false,
    }
}
fn reason_for(class: FlowClass) -> &'static str {
    match class {
        FlowClass::StatusInconsistent => "status_inconsistent",
        FlowClass::IdleWithFlow => "idle_with_flow",
        FlowClass::UnknownDirectionFlow => "unknown_direction_flow",
        FlowClass::HoldUnexpectedFlow => "hold_unexpected_flow",
        FlowClass::WrongDirection => "wrong_direction",
        FlowClass::UnknownDirectionNoFlow => "unknown_direction_no_flow",
        FlowClass::OppositeDirectionNoFlow => "opposite_direction_no_flow",
        FlowClass::HoldStatusDirectional => "hold_status_directional",
        FlowClass::HoldVerified => "hold_verified",
        FlowClass::NoFlow => "no_flow",
        FlowClass::FullFlow => "full_flow",
        FlowClass::PartialFlow => "partial_flow",
        FlowClass::OverFlow => "over_flow",
    }
}
fn measured_row(command: &PackCommand, report: &FlowReport, budget_ms: u64) -> DispatchRow {
    let mut row = DispatchRow::base(command, budget_ms);
    row.flow_measured = true;
    row = row.echo(report.echo, command.requested_w);
    row.applied_w = Some(report.measured_w);
    row.applied_reason = if report.measured_w == 0.0 {
        AppliedReason::IdleMeasured
    } else {
        AppliedReason::Measured
    };
    row
}
fn pending_row(
    command: &PackCommand,
    report: &FlowReport,
    class: FlowClass,
    provisional: bool,
    budget_ms: u64,
) -> DispatchRow {
    let mut row = measured_row(command, report, budget_ms)
        .outcome(
            DispatchOutcome::ConfirmationPending,
            EvidenceKind::Provisional,
        )
        .reason(reason_for(class));
    row.flow_class = Some(class);
    row.provisional = provisional;
    row.evidence_kind = if provisional {
        EvidenceKind::Provisional
    } else {
        EvidenceKind::CommandVerification
    };
    row
}
fn confirmed_row(
    command: &PackCommand,
    report: &FlowReport,
    class: FlowClass,
    budget_ms: u64,
) -> DispatchRow {
    let mut row = measured_row(command, report, budget_ms)
        .outcome(
            DispatchOutcome::Confirmed,
            EvidenceKind::CommandVerification,
        )
        .reason(reason_for(class));
    row.verified = true;
    row.confirmed_w = Some(command.requested_w);
    row.flow_class = Some(class);
    row
}
fn anomaly_row(
    command: &PackCommand,
    report: &FlowReport,
    class: FlowClass,
    budget_ms: u64,
    now_ms: i64,
) -> DispatchRow {
    let mut row = measured_row(command, report, budget_ms)
        .outcome(
            DispatchOutcome::ConfirmationPending,
            EvidenceKind::ConfirmedAnomaly,
        )
        .reason(reason_for(class));
    row.flow_class = Some(class);
    row.anomaly_confirmed = true;
    row.fault_backoff_incremented = true;
    row.retry_at = Some(now_ms + i64::try_from(NO_FLOW_BACKOFF_MS).unwrap_or(i64::MAX));
    row
}
fn shortfall_row(command: &PackCommand, report: &FlowReport, budget_ms: u64) -> DispatchRow {
    let mut row = measured_row(command, report, budget_ms)
        .outcome(
            DispatchOutcome::CapabilityShortfall,
            EvidenceKind::CapabilityObservation,
        )
        .reason(if command.requested_w < 0.0 {
            "discharge_derated_at_soc"
        } else {
            "charge_derated_at_soc"
        });
    row.delivered_w = Some(report.measured_w);
    row.shortfall_requested_w = Some(command.requested_w);
    row
}
fn apply_verdict(
    row: &mut DispatchRow,
    verdict: ConfirmationVerdict,
    command: &PackCommand,
    report: &FlowReport,
    budget_ms: u64,
    now_ms: i64,
) {
    *row = match verdict {
        ConfirmationVerdict::Pending { class, provisional } => {
            pending_row(command, report, class, provisional, budget_ms)
        }
        ConfirmationVerdict::Confirmed { class, .. } => {
            confirmed_row(command, report, class, budget_ms)
        }
        ConfirmationVerdict::Anomaly { class, .. } => {
            anomaly_row(command, report, class, budget_ms, now_ms)
        }
        ConfirmationVerdict::Shortfall { .. } => shortfall_row(command, report, budget_ms),
    };
}
fn apply_state(
    state: &mut crate::dispatch::PackState,
    report: &FlowReport,
    verdict: ConfirmationVerdict,
    now_ms: i64,
) {
    match verdict {
        ConfirmationVerdict::Confirmed { class, .. } => {
            let recovery = crate::dispatch::read_back::recovery_verdict(state, report, now_ms);
            if class == FlowClass::HoldVerified && report.echo.is_some() && recovery.is_none() {
                state.uncertain = false;
                state.uncertain_reason = None;
                state.uncertainty_cleared_by = Some("verified_hold");
            }
            state.pending = None;
        }
        ConfirmationVerdict::Anomaly { class, .. } => {
            let partial = class == FlowClass::PartialFlow;
            if !partial {
                state.pending = None;
                state.backoff_until_ms =
                    now_ms + i64::try_from(NO_FLOW_BACKOFF_MS).unwrap_or(i64::MAX);
                state.backoff_reason = Some(reason_for(class));
            }
            if partial && state.pause_until_ms < now_ms && state.partial_retries == 0 {
                state.pause_until_ms = now_ms + i64::try_from(PARTIAL_PAUSE_MS).unwrap_or(i64::MAX);
            }
        }
        ConfirmationVerdict::Shortfall { .. } => state.pending = None,
        ConfirmationVerdict::Pending { .. } => {}
    }
}

/// Fold one fresh report, producing a row even when no command is pending.
pub struct Observer<'a> {
    escalation: &'a crate::dispatch::observer_escalation::Escalation<'a>,
}

impl<'a> Observer<'a> {
    /// Build an observer from the one shared escalation context.
    pub fn new(escalation: &'a crate::dispatch::observer_escalation::Escalation<'a>) -> Self {
        Self { escalation }
    }

    /// Observe a report; no transport wait or confirmation sleep happens here.
    pub fn observe(
        &self,
        state: &mut DispatchState,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        let command = command_for(
            &report.pack_id,
            state.get(&report.pack_id).and_then(|p| p.pending.as_ref()),
        );
        let pack = state.pack(&report.pack_id);
        let Some(pending) = pack.pending.as_mut() else {
            return self.no_command(pack, &command, report, now_ms);
        };
        if pending.message_id != 0 && report.message_id <= pending.message_id {
            let mut row = DispatchRow::base(&command, self.escalation.budget_ms)
                .outcome(DispatchOutcome::Deferred, EvidenceKind::NoMeasurement)
                .reason("stale_report");
            row.applied_reason = AppliedReason::Deferred;
            return row;
        }
        pending.message_id = report.message_id;
        let verdict = fold(pending, report, now_ms, RAMP_MAX_MS);
        let was_uncertain = pack.uncertain;
        let recovery = crate::dispatch::read_back::recovery_verdict(pack, report, now_ms);
        apply_state(pack, report, verdict, now_ms);
        if pack.pause_until_ms > now_ms {
            pack.pause_target_w = Some(command.requested_w);
        }
        let mut row = DispatchRow::base(&command, self.escalation.budget_ms);
        apply_verdict(
            &mut row,
            verdict,
            &command,
            report,
            self.escalation.budget_ms,
            now_ms,
        );
        if was_uncertain && !pack.uncertain {
            row.uncertainty_cleared_by = Some("verified_hold");
        } else if let Some(reason) = recovery.filter(|_| was_uncertain) {
            row = row.reason(reason);
        }
        self.escalate(row, pack, &command, report, now_ms)
    }

    fn no_command(
        &self,
        pack: &mut crate::dispatch::PackState,
        command: &PackCommand,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        if !idle_confirmed(pack, report) {
            let mut row = DispatchRow::base(command, self.escalation.budget_ms)
                .outcome(DispatchOutcome::Deferred, EvidenceKind::NoMeasurement)
                .reason("one_neutral_report");
            row.applied_reason = AppliedReason::Deferred;
            return row;
        }
        let recovery = crate::dispatch::read_back::recovery_verdict(pack, report, now_ms);
        if report.echo.is_some() && recovery.is_none() {
            pack.uncertain = false;
            pack.uncertain_reason = None;
            pack.uncertainty_cleared_by = Some("verified_hold");
        }
        let mut row = confirmed_row(
            command,
            report,
            FlowClass::HoldVerified,
            self.escalation.budget_ms,
        )
        .reason("idle_verified")
        .reason("no_command_needed");
        if recovery.is_none() {
            row.uncertainty_cleared_by = Some("verified_hold");
        } else if pack.uncertain {
            row = row.reason(recovery.unwrap_or("application_causality_unknown"));
        }
        row
    }

    fn escalate(
        &self,
        row: DispatchRow,
        pack: &mut crate::dispatch::PackState,
        command: &PackCommand,
        report: &FlowReport,
        now_ms: i64,
    ) -> DispatchRow {
        crate::dispatch::observer_escalation::escalate(
            self.escalation,
            row,
            pack,
            command,
            report,
            now_ms,
        )
    }
}
