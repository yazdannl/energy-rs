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
