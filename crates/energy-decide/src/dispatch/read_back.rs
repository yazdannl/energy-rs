//! Causal, fresh and neutral validation for truth re-establishment.
//!
//! Transport contract: a physical application may occur after `write()` returns. M4
//! therefore assumes "yes" and requires evidence strictly after both the apply
//! attempt and the join of the future that carried the command.

use crate::dispatch::constants::{HOLD_DEADBAND_W, READBACK_FRESHNESS_MS};
use crate::dispatch::{FlowReport, FlowStatus, status_direction};

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

/// Apply a valid read-back to state, or annotate why evidence was rejected.
pub fn clear(
    mut row: crate::dispatch::DispatchRow,
    pack: &mut crate::dispatch::PackState,
    report: &FlowReport,
    now_ms: i64,
) -> crate::dispatch::DispatchRow {
    let evidence_after_ms = pack.last_apply_attempt_ms.max(pack.joined_at_ms);
    let Some(reason) = rejection(pack.transport_gone, report, now_ms, evidence_after_ms) else {
        pack.uncertain = false;
        pack.uncertain_reason = None;
        pack.uncertainty_cleared_by = Some("read_back");
        row = row
            .outcome(
                crate::dispatch::DispatchOutcome::Confirmed,
                crate::dispatch::EvidenceKind::CommandVerification,
            )
            .reason("read_back_verified");
        row.uncertainty_cleared_by = Some("read_back");
        return row;
    };
    row = row.reason(reason);
    row.applied_reason = crate::dispatch::AppliedReason::ReadbackRejected;
    row.controller_uncertain = pack.uncertain;
    row
}
