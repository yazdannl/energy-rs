//! Operator contract required for an explicit net-metering regime override.

use crate::horizon::AcknowledgedRegime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorOverride {
    /// The regime the operator actually acknowledged; must match the stored regime.
    pub regime: AcknowledgedRegime,
    /// Who acknowledged the non-default regime; must be non-empty.
    pub accepted_by: String,
    /// UTC epoch second after which this override no longer applies.
    pub expires_at: i64,
    /// Audit event identifying the acknowledgement; must be non-empty.
    pub event: String,
}
