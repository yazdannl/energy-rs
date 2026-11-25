//! Hash-chained ledger event owned by the authority module.

use crate::authority::AuthorityEventKind;

/// One durable authority event; checkpoint advances atomically with this append.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityEvent {
    /// Monotonic event id, also the ledger head.
    pub id: u64,
    /// Append instant, epoch milliseconds.
    pub at_ms: i64,
    /// Audited actor.
    pub actor: String,
    /// Audited reason.
    pub why: String,
    /// Narrow event payload; no device command is possible here.
    pub kind: AuthorityEventKind,
    /// Chain head before this event.
    pub prev_hash: u64,
    /// Chain head after this event.
    pub hash: u64,
}

impl AuthorityEvent {
    /// Build an event; the ledger assigns hashes when appending it.
    pub fn new(id: u64, at_ms: i64, actor: &str, why: &str, kind: AuthorityEventKind) -> Self {
        Self {
            id,
            at_ms,
            actor: actor.to_owned(),
            why: why.to_owned(),
            kind,
            prev_hash: 0,
            hash: 0,
        }
    }
}
