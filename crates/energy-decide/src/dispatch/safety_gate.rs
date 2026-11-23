//! Provisional M5 consumer port; M4 asks and obeys, it never decides safety.

use crate::dispatch::{SafetyIntervention, SlotRequest};

/// Mirrors the M5 safety decision into dispatch without reimplementing it.
pub trait SafetyGate: Send + Sync + 'static {
    /// Decide whether an economic request is replaced by a safety action.
    fn preempt(&self, request: &SlotRequest, now_ms: i64) -> SafetyIntervention;
}
