//! Consumed, non-cloneable proof of one sealed token/event commit.

use crate::authority::{AuthorityLedger, BootstrapReason};

/// Result of `BootstrapAuthorityTx::commit`; cannot be cloned or replayed.
#[derive(Debug)]
pub struct CommittedBootstrap {
    pub(crate) ledger: AuthorityLedger,
    pub(crate) token_hash: String,
    pub(crate) generation: u64,
    pub(crate) nonce: u64,
    pub(crate) reason: BootstrapReason,
    pub(crate) warning: String,
}

impl CommittedBootstrap {
    /// Only crate-internal startup code can unwrap the committed ledger.
    pub(crate) fn into_ledger(self) -> AuthorityLedger {
        self.ledger
    }
    /// Read-only snapshot for crash-boundary tests; consuming commit stays single-use.
    pub fn ledger_snapshot(&self) -> AuthorityLedger {
        self.ledger.clone()
    }
    /// Token digest that must be anchored before the gate opens.
    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }
    /// Generation proven by the ledger event.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Single-use event nonce.
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
    /// Audited bootstrap reason.
    pub fn reason(&self) -> BootstrapReason {
        self.reason
    }
    /// Prominent warning for audited advisory/unknown migration.
    pub fn warning(&self) -> &str {
        &self.warning
    }
}
