//! Answer from the provisional M5 consumer port.

use crate::dispatch::SafetyAction;

/// What M4 must do with an economic request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SafetyIntervention {
    /// No safety intervention; ordinary dispatch may continue.
    Proceed,
    /// M5 defers ordinary traffic; M4 records the reason and retry instant.
    Defer {
        /// Typed reason supplied by M5.
        reason: &'static str,
        /// First instant an ordinary retry is permitted.
        retry_at_ms: Option<i64>,
    },
    /// M5 owns the decision; M4 writes the requested action and records the reason.
    Hold {
        /// Action chosen by M5.
        action: SafetyAction,
        /// Typed reason supplied by M5.
        reason: &'static str,
    },
}
