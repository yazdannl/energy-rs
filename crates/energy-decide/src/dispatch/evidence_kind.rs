//! Evidence class attached to every row.

/// Where the row's claim came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// No measurement exists.
    NoMeasurement,
    /// Evidence collected before the ramp ended.
    Provisional,
    /// Fresh post-ramp reports confirming the issued command.
    CommandVerification,
    /// A measured anomaly, never a command verification.
    ConfirmedAnomaly,
    /// Measured shortfall at faultLevel 0.
    CapabilityObservation,
    /// Decision supplied by the M5 or M6 consumer port.
    PortDecision,
}
