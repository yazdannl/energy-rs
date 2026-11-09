//! Where the pool lives between ticks.

use energy_ledger::pool::PoolRow;

/// Where the pool lives between ticks.
pub trait PoolStore: Send + Sync {
    /// Load the pool; `None` before the first advance.
    fn load(&self) -> Option<PoolRow>;
    /// Persist the pool.
    fn save(&self, pool: &PoolRow);
}
