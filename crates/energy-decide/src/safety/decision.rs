//! One M5 decision: winner, all firing reasons and its verification record.

use crate::dispatch::SafetyAction;
use crate::safety::{DecisionOutcome, Escalation, Rule, SolarHeadroom};

/// Complete record of one ladder evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyDecision {
    /// Pack this decision belongs to.
    pub pack_id: String,
    /// Evaluation instant, epoch milliseconds.
    pub at_ms: i64,
    /// Winning rule; `Rule::Ordinary` means no safety command.
    pub rule: Rule,
    /// Terminal outcome.
    pub outcome: DecisionOutcome,
    /// Command M4 must write, when there is one.
    pub action: Option<SafetyAction>,
    /// Power M4 must write, W.
    pub command_w: f64,
    /// Reason code handed to M4.
    pub reason: &'static str,
    /// Every firing rule's reason, in precedence order.
    pub reasons: Vec<&'static str>,
    /// Physical limits that bound a blocked nonzero action.
    pub binding_limits: Vec<&'static str>,
    /// Effective target from M3's `max(soft, floor).min(ceiling)`.
    pub effective_target_kwh: f64,
    /// Raw soft guideline preserved for reporting.
    pub raw_guideline_kwh: f64,
    /// Advisory solar headroom; never a command or a charge ceiling.
    pub solar: SolarHeadroom,
    /// Verification requirement for the winning rule.
    pub verification: &'static str,
    /// Escalation path, when one applies.
    pub escalation: Option<Escalation>,
    /// Ordered evidence and escalation steps recorded for this decision.
    pub steps: Vec<&'static str>,
    /// First instant a retry is permitted, epoch milliseconds.
    pub retry_at_ms: Option<i64>,
}

impl SafetyDecision {
    /// Explicit no-action decision with the solar reason attached.
    pub fn no_action(
        pack_id: &str,
        at_ms: i64,
        solar: SolarHeadroom,
        effective_target_kwh: f64,
        raw_guideline_kwh: f64,
        reasons: Vec<&'static str>,
    ) -> Self {
        let mut reasons = reasons;
        reasons.insert(0, Rule::Ordinary.reason());
        Self {
            pack_id: pack_id.to_owned(),
            at_ms,
            rule: Rule::Ordinary,
            outcome: DecisionOutcome::NoAction,
            action: None,
            command_w: 0.0,
            reason: Rule::Ordinary.reason(),
            reasons,
            binding_limits: Vec::new(),
            effective_target_kwh,
            raw_guideline_kwh,
            solar,
            verification: Rule::Ordinary.verification(),
            escalation: None,
            steps: vec!["no_action_recorded", solar.reason],
            retry_at_ms: None,
        }
    }
}
