//! Freshness and neutrality validation for truth re-establishment.

use crate::dispatch::constants::{HOLD_DEADBAND_W, READBACK_FRESHNESS_MS};
use crate::dispatch::{FlowReport, FlowStatus, status_direction};

pub fn rejection(transport_gone: bool, report: &FlowReport, now_ms: i64) -> Option<&'static str> {
    if !transport_gone {
        return Some("transport_still_outstanding");
    }
    let echo = report.echo?;
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

/// Apply a valid read-back to state, or annotate why evidence was rejected.
pub fn clear(
    mut row: crate::dispatch::DispatchRow,
    pack: &mut crate::dispatch::PackState,
    report: &FlowReport,
    now_ms: i64,
) -> crate::dispatch::DispatchRow {
    let Some(reason) = rejection(pack.transport_gone, report, now_ms) else {
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
