//! Safety command execution derived from a row, independent of the M5 decision.

use crate::dispatch::{DispatchOutcome, DispatchRow, EvidenceKind, SafetyExecution};

pub fn execution(row: &DispatchRow) -> (SafetyExecution, EvidenceKind, Option<&'static str>) {
    if row.reasons.contains(&"write_deadline_mid_transport")
        || row.reasons.contains(&"late_after_deadline_superseded")
    {
        return (
            SafetyExecution::TimedOut,
            EvidenceKind::NoMeasurement,
            Some("write_deadline_mid_transport"),
        );
    }
    if row.outcome == DispatchOutcome::Rejected {
        return (
            SafetyExecution::Rejected,
            EvidenceKind::PortDecision,
            row.reasons.first().copied(),
        );
    }
    if row.outcome == DispatchOutcome::Uncertain {
        return (
            SafetyExecution::Failed,
            EvidenceKind::NoMeasurement,
            Some("controller_uncertain"),
        );
    }
    if row.echo_matched {
        return (
            SafetyExecution::Echoed,
            EvidenceKind::CommandVerification,
            None,
        );
    }
    (SafetyExecution::Accepted, EvidenceKind::PortDecision, None)
}

/// Annotate economic preemption without hiding the safety command's own outcome.
pub fn row(
    mut row: DispatchRow,
    gate_reason: Option<&'static str>,
    interrupted: bool,
) -> DispatchRow {
    if interrupted {
        row = row.reason("pause_interrupted_by_target_change");
    }
    if row.safety_traffic {
        let (execution, evidence, detail) = execution(&row);
        row.safety_execution = Some(execution);
        row.safety_evidence = Some(evidence);
        row.safety_reason = detail;
    }
    if let Some(reason) = gate_reason {
        row.economic_preempted_by = Some(reason);
        row = row.reason("safety_preempted").reason(reason);
    }
    row
}
