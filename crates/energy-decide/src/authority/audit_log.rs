//! Ordered authority audit rows and the observable gate-call count.

use crate::authority::{AuditEvent, EntryPoint};

/// Append-only audit log used to prove every entry point reached the one gate.
#[derive(Debug, Default)]
pub struct AuditLog {
    events: Vec<AuditEvent>,
    gate_calls: u64,
}

impl AuditLog {
    /// Record one gate decision and return the stored row.
    pub fn record(&mut self, mut event: AuditEvent) -> AuditEvent {
        self.gate_calls = self.gate_calls.saturating_add(1);
        event.seq = self.gate_calls;
        self.events.push(event.clone());
        event
    }

    /// All recorded decisions in order.
    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    /// Number of gate invocations; one per decision is the single-gate proof.
    pub fn gate_calls(&self) -> u64 {
        self.gate_calls
    }

    /// Refused decisions, including queued intents refused at execution time.
    pub fn refusals(&self) -> Vec<&AuditEvent> {
        self.events.iter().filter(|event| event.refused()).collect()
    }

    /// True when at least one decision for this entry point was observed.
    pub fn saw(&self, entry: EntryPoint) -> bool {
        self.events.iter().any(|event| event.entry == entry)
    }
}
