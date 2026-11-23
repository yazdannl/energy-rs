//! Execution outcome of a safety command, separate from the M5 decision.

/// What happened to the safety command M4 sent after M5 preempted the economic target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyExecution {
    /// Transport accepted the safety command; no echo seen yet.
    Accepted,
    /// Safety command's echo matched; flow is still a separate question.
    Echoed,
    /// Device explicitly rejected the safety command.
    Rejected,
    /// Transport failed before acceptance.
    Failed,
    /// Safety command timed out inside the per-pack budget.
    TimedOut,
}
