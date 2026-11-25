//! Durable issued marker that binds one bootstrap transaction to one live run.

use serde::{Deserialize, Serialize};

/// Written to the separate anchor before a token may be minted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapIssue {
    /// One transaction id per live run.
    pub tx_id: u64,
    /// Process/run identity; a previous run can never satisfy it.
    pub run_id: u64,
    /// Nonce bound to this issue, not derivable from ledger data alone.
    pub nonce: u64,
    /// Issue instant.
    pub at_ms: i64,
    /// True once the transaction committed.
    pub consumed: bool,
}

impl BootstrapIssue {
    /// Create an unconsumed issue marker.
    pub fn new(tx_id: u64, run_id: u64, nonce: u64, at_ms: i64) -> Self {
        Self {
            tx_id,
            run_id,
            nonce,
            at_ms,
            consumed: false,
        }
    }

    /// Mark this issue consumed after an atomic token/event commit.
    pub fn consume(&mut self) {
        self.consumed = true;
    }
}
