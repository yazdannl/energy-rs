//! Bounded transport result used by the per-pack coordinator.

use crate::dispatch::WriteAck;

/// Outcome of one transport attempt inside the single per-pack budget.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteAttempt {
    /// Transport returned within the budget.
    Ack {
        /// Transport answer.
        ack: WriteAck,
        /// Elapsed milliseconds inside the budget.
        elapsed_ms: u64,
    },
    /// Budget expired while the transport call was still running.
    Timeout {
        /// Elapsed milliseconds inside the budget.
        elapsed_ms: u64,
    },
    /// Completion arrived after the deadline had already superseded this write.
    Superseded {
        /// Monotonic generation that was ignored.
        generation: u64,
        /// Elapsed milliseconds inside the budget.
        elapsed_ms: u64,
    },
    /// `PackIo::write` exceeded its hard return-time contract; pack is latched.
    TransportContractViolated {
        /// Monotonic generation of the violating write.
        generation: u64,
        /// Elapsed milliseconds since the write started.
        elapsed_ms: u64,
    },
    /// Arbiter loop bound fired before this write reached its deadline.
    BoundExceeded {
        /// Monotonic generation left unresolved.
        generation: u64,
        /// Elapsed milliseconds inside the budget.
        elapsed_ms: u64,
    },
    /// Transport worker died without an answer.
    Failed {
        /// Typed reason.
        reason: &'static str,
        /// Elapsed milliseconds inside the budget.
        elapsed_ms: u64,
    },
}

impl WriteAttempt {
    /// Elapsed budget consumed by this attempt.
    pub fn elapsed_ms(&self) -> u64 {
        match self {
            Self::Ack { elapsed_ms, .. }
            | Self::Timeout { elapsed_ms }
            | Self::TransportContractViolated { elapsed_ms, .. }
            | Self::Superseded { elapsed_ms, .. }
            | Self::BoundExceeded { elapsed_ms, .. }
            | Self::Failed { elapsed_ms, .. } => *elapsed_ms,
        }
    }
}
