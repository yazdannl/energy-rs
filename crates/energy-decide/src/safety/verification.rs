//! Hold verification: echo opens the gate, measured flow ceasing is the verdict.

use crate::dispatch::FlowReport;
use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::safety::escalation::{Escalation, SAFETY_ATTEMPT_DEADLINE_MS};
use crate::safety::receipt::SafetyReceipt;

/// How long a neutral report stays fresh for hold verification.
pub const NEUTRAL_REPORT_FRESH_MS: i64 = 30_000;
/// Number of measured neutral reports required after a hold.
pub const NEUTRAL_REPORTS_REQUIRED: usize = 2;

/// Evidence state for one safety hold command.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldProof {
    /// Monotonic safety sequence for the command being proved.
    pub safety_seq: u64,
    /// Issue instant, epoch milliseconds.
    pub issued_at_ms: i64,
    /// Commanded power, W; a hold is 0 W.
    pub target_w: f64,
    /// Fresh attempt identifier; every retry gets a new one.
    pub attempt_id: u64,
    /// Matching echo seen for this command.
    pub echo_seen: bool,
    /// Timestamps of fresh measured neutral reports.
    pub neutral_reports: Vec<i64>,
    /// Ordered evidence/escalation steps recorded for this hold.
    pub steps: Vec<&'static str>,
    /// True only after echo and two measured neutral reports.
    pub verified: bool,
    /// Bounded escalation for this hold.
    pub escalation: Escalation,
}

impl HoldProof {
    /// Start proof for one issued safety command.
    pub fn new(safety_seq: u64, issued_at_ms: i64, target_w: f64) -> Self {
        Self {
            safety_seq,
            issued_at_ms,
            target_w,
            attempt_id: 1,
            echo_seen: false,
            neutral_reports: Vec::new(),
            steps: vec!["hold_issued"],
            verified: false,
            escalation: Escalation::new("ownership_retention"),
        }
    }

    /// Absolute deadline for this bounded attempt.
    pub fn deadline_ms(&self) -> i64 {
        self.issued_at_ms
            .saturating_add(i64::try_from(SAFETY_ATTEMPT_DEADLINE_MS).unwrap_or(i64::MAX))
    }

    /// Fold a correlated receipt without waiting. A stale attempt cannot verify anything.
    pub fn observe_receipt(&mut self, receipt: &SafetyReceipt, now_ms: i64) {
        if receipt.safety_seq != self.safety_seq || receipt.attempt_id != self.attempt_id {
            self.steps.push("late_uncorrelated");
            return;
        }
        self.apply_report(&receipt.report, now_ms);
    }

    fn apply_report(&mut self, report: &FlowReport, now_ms: i64) {
        if report.at_ms <= self.issued_at_ms
            || now_ms.saturating_sub(report.at_ms) > NEUTRAL_REPORT_FRESH_MS
        {
            self.steps.push("neutral_evidence_stale");
            return;
        }
        let had_echo = self.echo_seen;
        let echo = report.echo.filter(|echo| echo.matches(self.target_w));
        if echo.is_some() {
            self.echo_seen = true;
            self.steps.push("hold_echo_seen");
        }
        if !measured_neutral(report) {
            self.steps.push("flow_not_neutral");
            return;
        }
        if !had_echo {
            self.steps.push("neutral_before_echo_not_counted");
            return;
        }
        self.neutral_reports.push(report.at_ms);
        self.steps.push("neutral_flow_measured");
        if self.echo_seen && self.neutral_reports.len() >= NEUTRAL_REPORTS_REQUIRED {
            self.verified = true;
            self.steps.push("hold_verified_by_measured_flow");
        }
    }

    /// Record a failed attempt; three failures end in ownership retention.
    pub fn fail(&mut self, at_ms: i64, reason: &'static str) {
        self.escalation.fail(at_ms, reason);
        self.attempt_id = u64::from(self.escalation.failures.saturating_add(1));
        if !self.escalation.exhausted() {
            let index = usize::from(self.escalation.failures);
            let offset = crate::safety::escalation::SAFETY_RETRY_OFFSETS_MS
                .get(index)
                .copied()
                .unwrap_or(0);
            self.escalation.retry_at_ms = Some(
                self.issued_at_ms
                    .saturating_add(i64::try_from(offset).unwrap_or(i64::MAX)),
            );
        }
        self.steps.extend(self.escalation.steps.iter().copied());
    }
}

fn measured_neutral(report: &FlowReport) -> bool {
    report.measured_w.is_finite()
        && report.measured_w.abs() <= HOLD_DEADBAND_W
        && crate::dispatch::status_direction(report.ac_status, report.dc_status)
            == crate::dispatch::FlowStatus::Idle
        && report.device_clock_ok
        && report.device_clock_coherent
}
