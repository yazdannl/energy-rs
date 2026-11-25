//! The single gate's answer.

use crate::authority::AuthorityRefusal;

/// Authorized or refused with a typed reason the row and API can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    /// The write may proceed; this is not a device command itself.
    Authorized,
    /// The write is refused with a typed, auditable reason.
    Refused(AuthorityRefusal),
}

impl GateOutcome {
    /// True only when the gate authorized the write.
    pub fn authorized(&self) -> bool {
        matches!(self, Self::Authorized)
    }
}
