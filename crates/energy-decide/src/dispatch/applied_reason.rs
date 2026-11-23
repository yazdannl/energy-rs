//! Typed explanation of applied power, especially when it is null.

/// Why the applied power field has its value, including every null case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedReason {
    /// Fresh signed measurement, nonzero.
    Measured,
    /// Fresh measured idle within the 30 W deadband.
    IdleMeasured,
    /// Flow has not been measured yet.
    FlowNotMeasured,
    /// Transport accepted but the device echo was absent.
    EchoMissing,
    /// Read transport missed the flow report.
    ReadMiss,
    /// The per-pack budget expired during transport.
    TransportDeadline,
    /// Completion arrived after its generation was superseded and was ignored.
    LateSuperseded,
    /// Transport failed before acceptance.
    TransportFailed,
    /// Authority denied the write.
    AuthorityDenied,
    /// Safety port held the pack.
    SafetyHold,
    /// Deferred by backoff, pause or uncertainty.
    Deferred,
    /// Device explicitly rejected the command.
    Rejected,
}
