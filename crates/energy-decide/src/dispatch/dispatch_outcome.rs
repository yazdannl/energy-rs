//! Explicit dispatch row outcome; uncertainty and shortfall never collapse.

/// What M4 can honestly report for one pack in one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// Fresh, direction-consistent measured flow confirmed the command.
    Confirmed,
    /// Transport accepted and echo matched; measured flow is not available yet.
    ConfirmationPending,
    /// We do not know whether the device received the command; it may already be applied.
    /// No economic command may be sent until verified hold or read-back re-establishes truth.
    Uncertain,
    /// Device explicitly refused the command.
    Rejected,
    /// Device accepted but delivered materially less with faultLevel 0.
    CapabilityShortfall,
    /// Authority declined; no economic or safety command was written by M4.
    Skipped,
    /// Backoff, pause or uncertainty deferred the ordinary command.
    Deferred,
}
