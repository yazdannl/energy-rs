//! Deferred confirmation folding: echo gate first, measured flow is the verdict.

use crate::dispatch::constants::{RAMP_MAX_MS, SHORTFALL_SOC_MAX_PCT};
use crate::dispatch::flow_predicates::classify_flow;
use crate::dispatch::{FlowClass, FlowReport, PackState, PendingConfirmation, status_direction};

/// What one fresh report does to a pending command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmationVerdict {
    /// Still pending, with provisionality recorded.
    Pending {
        /// Ordered predicate.
        class: FlowClass,
        /// Whether this is pre-ramp evidence.
        provisional: bool,
    },
    /// Command confirmed by two in-band measurements.
    Confirmed {
        /// Ordered predicate.
        class: FlowClass,
        /// Fresh signed measurement.
        measured_w: f64,
    },
    /// Measured anomaly; never command verification.
    Anomaly {
        /// Ordered predicate.
        class: FlowClass,
        /// Fresh signed measurement.
        measured_w: f64,
    },
    /// Device accepted but delivered materially less with faultLevel 0.
    Shortfall {
        /// Fresh delivered power.
        measured_w: f64,
    },
}

fn is_shortfall(command_w: f64, report: &FlowReport, class: FlowClass) -> bool {
    command_w.abs() > 30.0
        && !report.faulted()
        && report.above_device_min_soc()
        && report.soc_pct <= SHORTFALL_SOC_MAX_PCT
        && matches!(class, FlowClass::PartialFlow | FlowClass::NoFlow)
}

fn note_band(pending: &mut PendingConfirmation, now_ms: i64) {
    if pending.first_in_band_ms.is_none() {
        pending.first_in_band_ms = Some(now_ms);
    } else {
        pending.samples_after_band = pending.samples_after_band.saturating_add(1);
    }
}

/// Fold one report into pending state; the ramp ends at the first in-band sample.
pub fn fold(
    pending: &mut PendingConfirmation,
    report: &FlowReport,
    now_ms: i64,
    ramp_max_ms: u64,
) -> ConfirmationVerdict {
    let class = classify_flow(
        pending.command_w,
        report.measured_w,
        status_direction(report.ac_status, report.dc_status),
    );
    if is_shortfall(pending.command_w, report, class) {
        return ConfirmationVerdict::Shortfall {
            measured_w: report.measured_w,
        };
    }
    match class {
        FlowClass::FullFlow | FlowClass::HoldVerified => {
            note_band(pending, now_ms);
            if pending.in_band_confirmed() {
                ConfirmationVerdict::Confirmed {
                    class,
                    measured_w: report.measured_w,
                }
            } else {
                ConfirmationVerdict::Pending {
                    class,
                    provisional: false,
                }
            }
        }
        FlowClass::PartialFlow => {
            partial_verdict(pending, now_ms, report.measured_w, class, ramp_max_ms)
        }
        FlowClass::NoFlow => {
            no_flow_verdict(pending, now_ms, report.measured_w, class, ramp_max_ms)
        }
        other => ConfirmationVerdict::Anomaly {
            class: other,
            measured_w: report.measured_w,
        },
    }
}

fn partial_verdict(
    pending: &mut PendingConfirmation,
    now_ms: i64,
    measured_w: f64,
    class: FlowClass,
    ramp_max_ms: u64,
) -> ConfirmationVerdict {
    let allowance = i64::try_from(ramp_max_ms).unwrap_or(i64::MAX);
    if pending.ramp_ended() || now_ms >= pending.issued_at_ms.saturating_add(allowance) {
        ConfirmationVerdict::Anomaly { class, measured_w }
    } else {
        ConfirmationVerdict::Pending {
            class,
            provisional: true,
        }
    }
}

fn no_flow_verdict(
    pending: &mut PendingConfirmation,
    now_ms: i64,
    measured_w: f64,
    class: FlowClass,
    ramp_max_ms: u64,
) -> ConfirmationVerdict {
    let deadline =
        pending.issued_at_ms + i64::try_from(ramp_max_ms.max(RAMP_MAX_MS)).unwrap_or(i64::MAX);
    if pending.ramp_ended() {
        return ConfirmationVerdict::Anomaly { class, measured_w };
    }
    if now_ms < deadline {
        return ConfirmationVerdict::Pending {
            class,
            provisional: true,
        };
    }
    pending.post_ramp_no_flow = pending.post_ramp_no_flow.saturating_add(1);
    if pending.post_ramp_no_flow >= 2 {
        ConfirmationVerdict::Anomaly { class, measured_w }
    } else {
        ConfirmationVerdict::Pending {
            class,
            provisional: false,
        }
    }
}

/// Whether a pack with no command has two fresh neutral reports.
pub fn idle_confirmed(state: &mut PackState, report: &FlowReport) -> bool {
    let neutral = status_direction(report.ac_status, report.dc_status)
        == crate::dispatch::FlowStatus::Idle
        && report.measured_w.abs() <= 30.0;
    if neutral {
        state.idle_reports = state.idle_reports.saturating_add(1);
    } else {
        state.idle_reports = 0;
    }
    state.idle_reports >= 2
}
