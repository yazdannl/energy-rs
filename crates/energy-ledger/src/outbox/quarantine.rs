//! One outbox row moved aside because it could not be decoded or journalled.

use crate::outbox::RawValue;

/// A preserved audit payload with the reason it could not be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedEvent {
    pub id: i64,
    pub source_id: i64,
    pub pack_id: RawValue,
    pub at: RawValue,
    pub summary: RawValue,
    pub reason: String,
    pub quarantined_at: i64,
}
