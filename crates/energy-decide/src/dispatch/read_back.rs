//! Device-timestamp causality and neutrality for truth re-establishment.
//!
//! No stopwatch can prove a late write has stopped applying. The only honest evidence
//! is a device-generated report whose timestamp is causally after the last apply
//! attempt and after the neutralising hold's acknowledgement. If the device clock is
//! unavailable, incoherent or cannot be ordered, the pack stays `Uncertain`.

use crate::dispatch::constants::{HOLD_DEADBAND_W, READBACK_FRESHNESS_MS};
use crate::dispatch::{
    AppliedReason, DispatchOutcome, DispatchRow, EvidenceKind, FlowReport, FlowStatus, PackCommand,
    PackState, status_direction,
};

/// Whether device ordering proves this report postdates every pending application.
pub fn causality_ok(report: &FlowReport, apply_attempt_ms: i64, neutral_ack_ms: i64) -> bool {
    report.device_clock_ok
        && report.device_clock_coherent
        && report.at_ms > apply_attempt_ms
        && report.at_ms > neutral_ack_ms
}

/// Reject missing, stale, non-causal, non-neutral or non-finite evidence.
pub fn rejection(
    transport_gone: bool,
    report: &FlowReport,
    now_ms: i64,
    evidence_after_ms: i64,
    neutral_ack_ms: i64,
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
    if !causality_ok(report, evidence_after_ms, neutral_ack_ms) {
        return Some("application_causality_unknown");
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
    let Some(reason) = rejection(
        pack.transport_gone,
        report,
        now_ms,
        evidence_after_ms,
        pack.last_neutral_ack_at_ms,
    ) else {
        pack.uncertain = false;
        pack.uncertain_reason = None;
        pack.uncertainty_cleared_by = Some("read_back");
        row = row
            .outcome(
                DispatchOutcome::Confirmed,
                EvidenceKind::CommandVerification,
            )
            .reason("read_back_verified");
        row.uncertainty_cleared_by = Some("read_back");
        return row;
    };
    row = row.reason(reason);
    row.applied_reason = AppliedReason::ReadbackRejected;
    row.controller_uncertain = pack.uncertain;
    row
}
