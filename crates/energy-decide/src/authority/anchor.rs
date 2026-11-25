//! Separate-file authority anchor: two-phase ledger head and startup-resolution reference.

use serde::{Deserialize, Serialize};

/// Anchor content. It lives in a separate file, never inside the ledger.
///
/// A coherent rollback of BOTH ledger and anchor (a full add-on snapshot restore) is locally
/// indistinguishable from a crash; see SPEC LIMITATIONS and the M6 amendment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityAnchor {
    /// Authority generation proven by the ledger initialization event.
    pub generation: u64,
    /// Digest of the durable latch token.
    pub token_hash: String,
    /// Ledger head known to have been committed.
    pub committed_head: u64,
    /// Prepared but not yet committed ledger head; zero means none.
    pub pending_head: u64,
    /// Hash chain head of the authority journal.
    pub journal_hash_chain_head: u64,
    /// Authority-owned checkpoint, advanced atomically with events.
    pub checkpoint_event_id: u64,
    /// Prepared startup-resolution record id.
    pub pending_startup_ref: Option<u64>,
    /// Durable startup-resolution record id.
    pub last_startup_resolution_ref: Option<u64>,
}

impl AuthorityAnchor {
    /// First-bootstrap anchor already carries its resolution reference in one atomic creation.
    pub fn new(
        generation: u64,
        token_hash: &str,
        committed_head: u64,
        chain_head: u64,
        checkpoint_event_id: u64,
        resolution_ref: u64,
    ) -> Self {
        Self {
            generation,
            token_hash: token_hash.to_owned(),
            committed_head,
            pending_head: 0,
            journal_hash_chain_head: chain_head,
            checkpoint_event_id,
            pending_startup_ref: None,
            last_startup_resolution_ref: Some(resolution_ref),
        }
    }

    /// Install a pending resolution reference without touching the durable one.
    pub fn prepare_ref(&mut self, record_id: u64) {
        self.pending_startup_ref = Some(record_id);
    }

    /// Promote the pending reference after the record and directory entries are durable.
    pub fn promote_ref(&mut self) -> bool {
        match self.pending_startup_ref {
            Some(record_id) => {
                self.last_startup_resolution_ref = Some(record_id);
                self.pending_startup_ref = None;
                true
            }
            None => false,
        }
    }

    /// Discard an uncommitted prepare; used by the prepare-abort crash case.
    pub fn discard_pending(&mut self) {
        self.pending_head = 0;
    }

    /// Adopt a ledger head that matched the pending head and promote it.
    pub fn adopt_pending(&mut self, head: u64) {
        self.committed_head = head;
        self.pending_head = 0;
    }
}
