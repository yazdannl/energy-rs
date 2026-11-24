//! M5 decision joined to M4's honest safety-execution facts.

use crate::dispatch::{DispatchRow, EvidenceKind, SafetyExecution, safety_execution_row};
use crate::safety::SafetyDecision;

/// Decision plus the independent execution result of the safety command.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyReport {
    /// M5's decision, including ordered rules and escalation steps.
    pub decision: SafetyDecision,
    /// M4's safety command execution, never inferred from the decision.
    pub execution: Option<SafetyExecution>,
    /// Evidence class attached to the safety command, when one was sent.
    pub evidence: Option<EvidenceKind>,
    /// Typed detail for the execution result.
    pub reason: Option<&'static str>,
    /// Complete ordered record: rule reasons, binding limits, escalation and execution.
    pub steps: Vec<&'static str>,
}

/// Join an M4 row to an M5 decision without hiding a failed safety write.
pub fn from_row(row: &DispatchRow, decision: &SafetyDecision) -> SafetyReport {
    let mut steps = decision.steps.clone();
    let (execution, evidence, reason) = safety_execution_row::execution(row);
    let authority_denied = row.reasons.contains(&"authority_denied");
    let (execution, evidence, reason) = if authority_denied {
        (None, None, reason)
    } else {
        steps.push("safety_execution_recorded");
        if let Some(reason) = reason {
            steps.push(reason);
        }
        (Some(execution), Some(evidence), reason)
    };
    SafetyReport {
        decision: decision.clone(),
        execution,
        evidence,
        reason,
        steps,
    }
}
