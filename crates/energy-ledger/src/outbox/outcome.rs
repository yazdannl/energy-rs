//! What happened when a pending outbox row was published.

/// Result of one publish attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// The row was claimed and its `learning.applied` event was written.
    Published,
    /// Another publisher already claimed the row; no second event was written.
    AlreadyClaimed,
    /// The row was poison; its payload is in the quarantine table and an alert was journalled.
    Quarantined,
}
