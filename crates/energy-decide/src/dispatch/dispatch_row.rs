//! The single observable record of what happened to one pack.

use crate::dispatch::{AppliedReason, DispatchOutcome, Echo, EvidenceKind, FlowClass, PackCommand};

/// Per-pack row; every distinct M4 state is distinguishable here.
// Independent transport/verification/flow facts are a reviewer requirement, not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchRow {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Explicit outcome state.
    pub outcome: DispatchOutcome,
    /// Requested power in W, charge positive.
    pub requested_w: f64,
    /// Power accepted by transport, when there was an acknowledgement.
    pub written_w: Option<f64>,
    /// Power confirmed by post-ramp reports, when confirmed.
    pub confirmed_w: Option<f64>,
    /// Fresh signed measurement only; null with `applied_reason` otherwise.
    pub applied_w: Option<f64>,
    /// Typed explanation of `applied_w`.
    pub applied_reason: AppliedReason,
    /// Transport acknowledgement.
    pub written: bool,
    /// Command verification only, never set by an anomaly or echo.
    pub verified: bool,
    /// Whether a fresh reading exists.
    pub flow_measured: bool,
    /// Device echo seen.
    pub echo_seen: bool,
    /// Echo matched the command.
    pub echo_matched: bool,
    /// Anomaly confirmation, independent of `verified`.
    pub anomaly_confirmed: bool,
    /// Evidence collected during the ramp.
    pub provisional: bool,
    /// Evidence class.
    pub evidence_kind: EvidenceKind,
    /// Ordered predicates that apply, first match first.
    pub flow_class: Option<FlowClass>,
    /// Ordered typed reasons.
    pub reasons: Vec<&'static str>,
    /// M4-owned end-to-end budget, milliseconds.
    pub budget_ms: u64,
    /// Time spent in apply + echo + fail-safe, milliseconds.
    pub elapsed_ms: u64,
    /// Next permitted ordinary retry, epoch milliseconds.
    pub retry_at: Option<i64>,
    /// Whether M5 classified this traffic as safety.
    pub safety_traffic: bool,
    /// Whether this pack is uncertain and ordinary retries are blocked.
    pub controller_uncertain: bool,
    /// Whether fault backoff was incremented by this row.
    pub fault_backoff_incremented: bool,
    /// Absolute retry offsets used by a fail-safe hold, milliseconds.
    pub retry_offsets_ms: Vec<u64>,
    /// Delivered W observed for a capability shortfall.
    pub delivered_w: Option<f64>,
    /// Requested W for a capability shortfall.
    pub shortfall_requested_w: Option<f64>,
}

impl DispatchRow {
    /// Row with honest defaults; a caller must set the outcome explicitly.
    pub fn base(command: &PackCommand, budget_ms: u64) -> Self {
        Self {
            pack_id: command.pack_id.clone(),
            outcome: DispatchOutcome::ConfirmationPending,
            requested_w: command.requested_w,
            written_w: None,
            confirmed_w: None,
            applied_w: None,
            applied_reason: AppliedReason::FlowNotMeasured,
            written: false,
            verified: false,
            flow_measured: false,
            echo_seen: false,
            echo_matched: false,
            anomaly_confirmed: false,
            provisional: false,
            evidence_kind: EvidenceKind::NoMeasurement,
            flow_class: None,
            reasons: Vec::new(),
            budget_ms,
            elapsed_ms: 0,
            retry_at: None,
            safety_traffic: command.safety_traffic,
            controller_uncertain: false,
            fault_backoff_incremented: false,
            retry_offsets_ms: Vec::new(),
            delivered_w: None,
            shortfall_requested_w: None,
        }
    }

    /// Set the row outcome and matching evidence kind.
    #[must_use]
    pub fn outcome(mut self, outcome: DispatchOutcome, kind: EvidenceKind) -> Self {
        self.outcome = outcome;
        self.evidence_kind = kind;
        self
    }

    /// Record the echo as a preliminary gate only.
    #[must_use]
    pub fn echo(mut self, echo: Option<Echo>, command_w: f64) -> Self {
        self.echo_seen = echo.is_some();
        self.echo_matched = echo.is_some_and(|value| value.matches(command_w));
        self
    }

    /// Add an ordered reason.
    #[must_use]
    pub fn reason(mut self, reason: &'static str) -> Self {
        self.reasons.push(reason);
        self
    }
}
