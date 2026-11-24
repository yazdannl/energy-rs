//! Causal, fresh and neutral validation for truth re-establishment.
//!
//! Transport contract: a physical application may occur after `write()` returns. M4
//! assumes yes and never treats a join as physical completion. Clearance needs two
//! consecutive neutral observations spanning `MAX_APPLICATION_DELAY_MS`, or a verified
//! safety hold over the same window. An uncertain pack with neither exit is a defect,
//! not a safe final state.

use crate::dispatch::constants::{
    HOLD_DEADBAND_W, MAX_APPLICATION_DELAY_MS, READBACK_FRESHNESS_MS,
};
use crate::dispatch::{
    AppliedReason, DispatchOutcome, DispatchRow, EvidenceKind, FlowReport, FlowStatus, PackCommand,
    PackState, status_direction,
};

/// Reject missing, stale, non-causal, non-neutral or non-finite evidence.
pub fn rejection(
    transport_gone: bool,
    report: &FlowReport,
    now_ms: i64,
    evidence_after_ms: i64,
) -> Option<&'static str> {
    if !transport_gone {
        return Some("transport_still_outstanding");
    }
    let Some(echo) = report.echo else {
        return Some("readback_echo_missing");
    };
    if !report.measured_w.is_finite() || !echo.limit_w.is_finite() {
        return Some("readback_non_finite");
    }
    let fresh = |at_ms: i64| {
        at_ms <= now_ms
            && now_ms.saturating_sub(at_ms)
                <= i64::try_from(READBACK_FRESHNESS_MS).unwrap_or(i64::MAX)
    };
    if !fresh(report.at_ms) {
        return Some("readback_stale_report");
    }
    if !fresh(echo.at_ms) {
        return Some("readback_stale_echo");
    }
    if report.at_ms <= evidence_after_ms || echo.at_ms <= evidence_after_ms {
        return Some("readback_not_causal");
    }
    if echo.limit_w.abs() > HOLD_DEADBAND_W {
        return Some("readback_not_neutral");
    }
    if report.measured_w.abs() > HOLD_DEADBAND_W
        || status_direction(report.ac_status, report.dc_status) != FlowStatus::Idle
        || report.fault_level != 0
    {
        return Some("readback_not_idle");
    }
    None
}

/// Base row for a read-back, built only from tracked command identity.
pub fn row(pack: &PackState, report: &FlowReport, budget_ms: u64) -> DispatchRow {
    let command = pack.last_command.clone().unwrap_or(PackCommand {
        pack_id: report.pack_id.clone(),
        requested_w: 0.0,
        issued_at_ms: report.at_ms,
        generation: 0,
        fence_id: None,
        safety_traffic: false,
    });
    DispatchRow::base(&command, budget_ms)
        .outcome(DispatchOutcome::Uncertain, EvidenceKind::NoMeasurement)
        .reason("read_back")
        .reason("controller_uncertain")
}

/// Apply a valid read-back to state, or annotate why evidence was rejected.
pub fn clear(
    mut row: DispatchRow,
    pack: &mut PackState,
    report: &FlowReport,
    now_ms: i64,
) -> DispatchRow {
    let evidence_after_ms = pack.last_apply_attempt_ms.max(pack.joined_at_ms);
    let Some(reason) = rejection(pack.transport_gone, report, now_ms, evidence_after_ms) else {
        let bound = i64::try_from(MAX_APPLICATION_DELAY_MS).unwrap_or(i64::MAX);
        let start = pack.neutral_window_start_ms.get_or_insert(report.at_ms);
        pack.neutral_window_last_ms = report.at_ms;
        if report.at_ms <= start.saturating_add(bound) {
            let mut pending = row.reason("application_bound_not_covered");
            pending.applied_reason = AppliedReason::ReadbackRejected;
            pending.controller_uncertain = pack.uncertain;
            return pending;
        }
        pack.uncertain = false;
        pack.uncertain_reason = None;
        pack.uncertainty_cleared_by = Some("read_back");
        pack.neutral_window_start_ms = None;
        pack.neutral_window_last_ms = 0;
        row = row
            .outcome(
                DispatchOutcome::Confirmed,
                EvidenceKind::CommandVerification,
            )
            .reason("read_back_verified");
        row.uncertainty_cleared_by = Some("read_back");
        return row;
    };
    pack.neutral_window_start_ms = None;
    pack.neutral_window_last_ms = 0;
    row = row.reason(reason);
    row.applied_reason = AppliedReason::ReadbackRejected;
    row.controller_uncertain = pack.uncertain;
    row
}
