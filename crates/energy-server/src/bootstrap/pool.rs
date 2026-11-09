//! Ledger-backed cost pool and settings-backed policy source.

use std::sync::Arc;

use energy_core::savings::PoolStore;
use energy_ledger::Ledger;
use energy_ledger::pool::PoolRow;

/// The pool as it lives in `/data/energy_rs.db`.
pub struct LedgerPool {
    ledger: Arc<Ledger>,
}

impl LedgerPool {
    /// A store over `ledger`.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }

    /// A shared handle.
    pub fn shared(ledger: Arc<Ledger>) -> Arc<Self> {
        Arc::new(Self::new(ledger))
    }
}

impl PoolStore for LedgerPool {
    fn load(&self) -> Option<PoolRow> {
        self.ledger
            .pool()
            .ok()
            .and_then(|pool| pool.get().ok().flatten())
    }

    fn save(&self, pool: &PoolRow) {
        if let Ok(repo) = self.ledger.pool() {
            let _ = repo.set(pool);
        }
    }
}
