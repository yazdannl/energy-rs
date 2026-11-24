//! Safety receipt correlation: both sequence and attempt must match.

use crate::dispatch::FlowReport;

/// A measured report attributed to one safety command attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyReceipt {
    /// Pack this receipt belongs to.
    pub pack_id: String,
    /// Monotonic safety sequence of the command.
    pub safety_seq: u64,
    /// Fresh attempt identifier for this retry.
    pub attempt_id: u64,
    /// Measured report, including echo and flow.
    pub report: FlowReport,
    /// Independently observed cell threshold result for an emergency charge.
    pub emergency_resolved: Option<bool>,
}
