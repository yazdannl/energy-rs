//! Answer from the provisional M5 consumer port.

use crate::dispatch::SafetyAction;

/// What M4 must do with an economic request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SafetyIntervention {
    /// No safety intervention; ordinary dispatch may continue.
    Proceed,
    /// M5 owns the decision; M4 writes the requested action and records the reason.
    Hold {
        /// Action chosen by M5.
        action: SafetyAction,
        /// Typed reason supplied by M5.
        reason: &'static str,
    },
}
