//! Common terminal envelope for one M5 decision.

/// Explicit terminal outcome; a tick never ends with only an absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionOutcome {
    /// A safety command was selected.
    Commanded,
    /// A requested nonzero safety action was impossible; a neutral hold is commanded.
    Blocked,
    /// Ordinary traffic waits for a named retry or verification.
    Deferred,
    /// No safety command is needed; the economic request proceeds.
    NoAction,
    /// Safety lane could not establish neutrality after its bounded attempts.
    Failed,
}
