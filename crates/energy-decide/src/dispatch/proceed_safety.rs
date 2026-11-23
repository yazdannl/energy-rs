//! Default safety port for tests and unwired modules.

use crate::dispatch::{SafetyGate, SafetyIntervention, SlotRequest};

/// Safety gate that never intervenes; real M5 replaces it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProceedSafety;

impl SafetyGate for ProceedSafety {
    fn preempt(&self, _request: &SlotRequest, _now_ms: i64) -> SafetyIntervention {
        SafetyIntervention::Proceed
    }
}
