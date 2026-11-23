//! Persistent partial-flow escalation, kept separate from report folding.

use std::sync::{Arc, Mutex};

use crate::dispatch::constants::PARTIAL_PAUSE_MS;
use crate::dispatch::fallback::{fallback_accepted, write_fallback};
use crate::dispatch::flow_predicates::classify_flow;
use crate::dispatch::{
    DispatchContext, DispatchOutcome, DispatchRow, DriveBoundEvent, EvidenceKind, FallbackResult,
    FlowClass, FlowReport, LateWrite, PackCommand, PackIo, PackState, RetryClock, status_direction,
};

/// Transport and clock used only when a partial flow becomes persistent.
pub struct Escalation<'a> {
    /// Shared transport.
    pub io: &'a Arc<dyn PackIo>,
    /// Retry clock for fail-safe offsets.
    pub clock: &'a dyn RetryClock,
    /// Late completions from superseded writes.
    pub late: &'a Arc<Mutex<Vec<LateWrite>>>,
    /// Driver-loop bound violations.
    pub bound: &'a Arc<Mutex<Vec<DriveBoundEvent>>>,
    /// Arbiter poll bound.
    pub max_polls: u64,
    /// End-to-end per-pack budget.
    pub budget_ms: u64,
}

/// Escalate a confirmed partial-flow anomaly after its second consecutive report.
pub fn escalate(
    escalation: &Escalation<'_>,
    row: DispatchRow,
    pack: &mut PackState,
    command: &PackCommand,
    report: &FlowReport,
    now_ms: i64,
) -> DispatchRow {
    if row.flow_class != Some(FlowClass::PartialFlow) || !row.anomaly_confirmed {
        return row;
    }
    let class = classify_flow(
        command.requested_w,
        report.measured_w,
        status_direction(report.ac_status, report.dc_status),
    );
    if class != FlowClass::PartialFlow || now_ms < pack.pause_until_ms {
        return row;
    }
    pack.partial_retries = pack.partial_retries.saturating_add(1);
    if pack.partial_retries < 2 {
        return pause_row(row, now_ms);
    }
    let context = DispatchContext {
        io: escalation.io,
        clock: escalation.clock,
        late: escalation.late,
        bound: escalation.bound,
        max_polls: escalation.max_polls,
        budget_ms: escalation.budget_ms,
        now_ms,
    };
    let fallback: FallbackResult = write_fallback(&context, command, escalation.budget_ms);
    let reason = if fallback_accepted(&fallback) {
        "fallback_hold_written"
    } else {
        "safety_hold_unverified"
    };
    row.outcome(DispatchOutcome::Deferred, EvidenceKind::PortDecision)
        .reason("partial_flow_escalated")
        .reason(reason)
}

fn pause_row(row: DispatchRow, now_ms: i64) -> DispatchRow {
    let mut row = row
        .outcome(DispatchOutcome::Deferred, EvidenceKind::PortDecision)
        .reason("persistent_partial")
        .reason("retry_pending");
    row.retry_at = Some(now_ms + i64::try_from(PARTIAL_PAUSE_MS).unwrap_or(i64::MAX));
    row
}
