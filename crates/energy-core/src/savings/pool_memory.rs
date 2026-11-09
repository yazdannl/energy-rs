//! A process-local pool store.

use std::sync::Arc;

use energy_ledger::pool::PoolRow;

use crate::savings::PoolStore;

/// A process-local pool store (tests, advisory runs).
#[derive(Debug, Default)]
pub struct MemoryPoolStore {
    pool: std::sync::Mutex<Option<PoolRow>>,
}

impl MemoryPoolStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// A shared handle.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl PoolStore for MemoryPoolStore {
    fn load(&self) -> Option<PoolRow> {
        self.pool.lock().ok().and_then(|pool| *pool)
    }

    fn save(&self, pool: &PoolRow) {
        if let Ok(mut slot) = self.pool.lock() {
            *slot = Some(*pool);
        }
    }
}
