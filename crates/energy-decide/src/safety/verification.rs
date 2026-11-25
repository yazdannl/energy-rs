//! Hold verification: echo opens the gate, measured flow ceasing is the verdict.

use crate::dispatch::FlowReport;
use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::safety::escalation::Escalation;
use crate::safety::flow_evidence::measured_neutral;
use crate::safety::input::FUTURE_SKEW_MS;
use crate::safety::receipt::SafetyReceipt;

/// How long a neutral report stays fresh for hold verification.
pub const NEUTRAL_REPORT_FRESH_MS: i64 = 30_000;
/// Number of distinct, ordered measured neutral reports required after a hold.
pub const NEUTRAL_REPORTS_REQUIRED: usize = 2;

/// Lifecycle of the current bounded attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldAttemptState {
    /// Receipts are accepted for this attempt.
    Active,
    /// Failure recorded; the next attempt waits for its retry instant.
    AwaitingRetry,
    /// Terminal escalation, retained until explicit recovery.
    Terminal,
}

/// Evidence state for one safety command attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldProof {
    /// Monotonic safety sequence for the command being proved.
    pub safety_seq: u64,
    /// Issue instant of the first attempt, epoch milliseconds.
    pub issued_at_ms: i64,
    /// Commanded power, W; a neutral hold is 0 W.
    pub target_w: f64,
    /// Fresh attempt identifier; every retry gets a new one.
    pub attempt_id: u64,
    /// Issue instant of the current attempt, epoch milliseconds.
    pub attempt_issued_at_ms: i64,
    /// Bounded duration of one attempt, milliseconds.
    pub attempt_duration_ms: i64,
    /// Matching echo seen for the current attempt.
    pub echo_seen: bool,
    /// Distinct, ordered timestamps of fresh measured neutral reports.
    pub neutral_reports: Vec<i64>,
    /// Last report seen for the current attempt, ordered by measurement time.
    pub last_report_at_ms: Option<i64>,
    /// Last neutral report counted toward verification.
    pub last_neutral_at_ms: Option<i64>,
    /// Higher-priority reason that explicitly superseded this proof.
    pub superseded_by: Option<&'static str>,
    /// True only after echo and two measured neutral reports.
    pub verified: bool,
    /// Current bounded-attempt lifecycle state.
    pub(crate) attempt_state: HoldAttemptState,
    /// Ordered evidence/escalation steps recorded for this hold.
    pub steps: Vec<&'static str>,
    /// Bounded escalation for this hold.
    pub escalation: Escalation,
    /// Escalation history retained from a superseded nonzero proof.
    pub prior_escalation: Vec<&'static str>,
}

impl HoldProof {
    /// Start proof for one issued safety command.
    pub fn new(safety_seq: u64, issued_at_ms: i64, target_w: f64, duration_ms: i64) -> Self {
        Self {
            safety_seq,
            issued_at_ms,
            target_w,
            attempt_id: 1,
            attempt_issued_at_ms: issued_at_ms,
            attempt_duration_ms: duration_ms,
            echo_seen: false,
            neutral_reports: Vec::new(),
            last_report_at_ms: None,
            last_neutral_at_ms: None,
            superseded_by: None,
            verified: false,
            attempt_state: HoldAttemptState::Active,
            steps: vec!["hold_issued"],
            escalation: Escalation::new("ownership_retention"),
            prior_escalation: Vec::new(),
        }
    }

    /// Absolute deadline for the current attempt.
    pub fn deadline_ms(&self) -> i64 {
        self.attempt_issued_at_ms
            .saturating_add(self.attempt_duration_ms)
    }

    /// This proof still blocks ordinary traffic until verified or superseded.
    pub fn blocks_ordinary(&self) -> bool {
        !self.verified && self.superseded_by.is_none()
    }

    /// Advance elapsed retries and deadlines from the decision clock, not an external call.
    pub fn advance_due(&mut self, now_ms: i64) -> Option<&'static str> {
        if self.verified || self.superseded_by.is_some() {
            return None;
        }
        match self.attempt_state {
            HoldAttemptState::Active if now_ms >= self.deadline_ms() => {
                self.fail(now_ms, "safety_attempt_deadline");
                Some("safety_attempt_deadline")
            }
            HoldAttemptState::AwaitingRetry
                if self.escalation.retry_at_ms.is_some_and(|at| now_ms >= at) =>
            {
                self.start_attempt(now_ms);
                Some("safety_retry_started")
            }
            _ => None,
        }
    }

    /// Fold a correlated receipt without waiting. A stale attempt cannot verify anything.
    pub fn observe_receipt(&mut self, receipt: &SafetyReceipt, now_ms: i64) {
        self.advance_due(now_ms);
        if receipt.safety_seq != self.safety_seq || receipt.attempt_id != self.attempt_id {
            self.steps.push("late_uncorrelated");
            return;
        }
        if self.superseded_by.is_some() {
            self.steps.push("superseded_attempt");
            return;
        }
        if self.attempt_state != HoldAttemptState::Active || now_ms < self.attempt_issued_at_ms {
            self.steps.push("attempt_not_active");
            return;
        }
        if receipt.report.at_ms > now_ms.saturating_add(FUTURE_SKEW_MS) {
            self.steps.push("future_dated_report");
            return;
        }
        self.apply_report(&receipt.report, receipt.emergency_resolved, now_ms);
    }

    fn apply_report(&mut self, report: &FlowReport, emergency_resolved: Option<bool>, now_ms: i64) {
        if self.last_report_at_ms == Some(report.at_ms) {
            self.steps.push("duplicate_neutral_report");
            return;
        }
        if self
            .last_report_at_ms
            .is_some_and(|last| report.at_ms < last)
        {
            self.steps.push("non_chronological_report");
            return;
        }
        self.last_report_at_ms = Some(report.at_ms);
        if report.at_ms < self.attempt_issued_at_ms
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
        if self.target_w > HOLD_DEADBAND_W {
            self.apply_emergency(report, emergency_resolved);
            return;
        }
        if !measured_neutral(report) {
            self.invalidate_neutral_run();
            self.steps.push("flow_not_neutral");
            return;
        }
        if !had_echo {
            self.steps.push("neutral_before_echo_not_counted");
            return;
        }
        if self
            .last_neutral_at_ms
            .is_some_and(|last| report.at_ms <= last)
        {
            self.steps.push("duplicate_neutral_report");
            return;
        }
        self.neutral_reports.push(report.at_ms);
        self.last_neutral_at_ms = Some(report.at_ms);
        self.steps.push("neutral_flow_measured");
        if self.neutral_reports.len() >= NEUTRAL_REPORTS_REQUIRED {
            self.verified = true;
            self.steps.push("hold_verified_by_measured_flow");
        }
    }

    fn apply_emergency(&mut self, report: &FlowReport, emergency_resolved: Option<bool>) {
        if emergency_resolved == Some(true)
            && self.echo_seen
            && report.measured_w.is_finite()
            && report.measured_w >= self.target_w * 0.8
        {
            self.verified = true;
            self.steps.push("emergency_charge_verified");
        } else {
            self.steps.push("emergency_not_resolved");
        }
    }

    fn invalidate_neutral_run(&mut self) {
        if self.verified {
            self.verified = false;
            self.echo_seen = false;
            self.steps.push("hold_reprove_required");
        }
        if !self.neutral_reports.is_empty() {
            self.neutral_reports.clear();
            self.last_neutral_at_ms = None;
            self.steps.push("neutral_run_invalidated");
        }
    }

    /// Record a failed attempt; three failures end in ownership retention.
    pub fn fail(&mut self, at_ms: i64, reason: &'static str) {
        if self.verified || self.superseded_by.is_some() {
            return;
        }
        if self.attempt_state == HoldAttemptState::AwaitingRetry {
            if self
                .escalation
                .retry_at_ms
                .is_some_and(|retry_at| at_ms >= retry_at)
            {
                self.start_attempt(at_ms);
            } else {
                return;
            }
        }
        if self.attempt_state != HoldAttemptState::Active {
            return;
        }
        self.escalation.fail(at_ms, reason);
        self.steps.push(reason);
        self.attempt_id = self.attempt_id.saturating_add(1);
        self.reset_attempt_evidence();
        if self.escalation.exhausted() {
            self.attempt_state = HoldAttemptState::Terminal;
        } else {
            self.attempt_state = HoldAttemptState::AwaitingRetry;
        }
        if let Some(retry_at) = self.escalation.retry_at_ms {
            self.steps.push("safety_retry_scheduled");
            let _ = retry_at;
        }
        self.steps.extend(
            self.escalation
                .steps
                .iter()
                .filter(|step| **step != reason)
                .copied(),
        );
    }

    /// A higher-priority safety action owns the pack; this proof stops blocking.
    pub fn supersede(&mut self, reason: &'static str) {
        if self.superseded_by.is_none() {
            self.superseded_by = Some(reason);
            self.verified = false;
            self.reset_attempt_evidence();
            self.steps.push("hold_superseded");
        }
    }

    /// Explicit recovery clears terminal retention and opens a bounded new proof.
    pub fn recover(&mut self, at_ms: i64) {
        self.steps.push("hold_recovered");
        *self = Self::new(
            self.safety_seq.saturating_add(1),
            at_ms,
            self.target_w,
            self.attempt_duration_ms,
        );
        self.steps.push("hold_recovered");
    }

    fn start_attempt(&mut self, now_ms: i64) {
        self.attempt_issued_at_ms = now_ms;
        self.attempt_state = HoldAttemptState::Active;
        self.reset_attempt_evidence();
        self.steps.push("safety_retry_started");
    }

    fn reset_attempt_evidence(&mut self) {
        self.echo_seen = false;
        self.neutral_reports.clear();
        self.last_report_at_ms = None;
        self.last_neutral_at_ms = None;
    }
}
