//! One outbox row moved aside because it could not be decoded or journalled.

/// A preserved audit payload with the reason it could not be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedEvent {
    pub id: i64,
    pub source_id: i64,
    pub pack_id: String,
    pub at: i64,
    pub summary: String,
    pub raw_pack_id: String,
    pub raw_at: String,
    pub raw_summary: String,
    pub reason: String,
    pub quarantined_at: i64,
}
