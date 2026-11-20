//! Durable per-pack balancing history.

use energy_ledger::LedgerError;

/// The small part of the ledger the maintenance rule owns.
///
/// The interval clock starts only when a dwell *completes*, so the three writes are deliberately
/// separate: a pack that merely passes 99 % has `dwell_started_at` but no `last_full_at`.
pub trait FullChargeHistory: Send + Sync {
    /// Last completed dwell, UTC epoch seconds.
    fn last_full_at(&self, pack_id: &str) -> Option<i64>;

    /// Start of the current dwell, UTC epoch seconds.
    fn dwell_started_at(&self, pack_id: &str) -> Option<i64>;

    /// Record the start of a dwell, at most once per run.
    fn begin_dwell(&self, pack_id: &str, at: i64) -> Result<(), LedgerError>;

    /// Stamp a completed dwell and clear its start.
    fn finish_dwell(&self, pack_id: &str, at: i64) -> Result<(), LedgerError>;

    /// Drop a dwell that did not complete.
    fn abort_dwell(&self, pack_id: &str) -> Result<(), LedgerError>;

    /// Packs with an unconfirmed dwell in storage; used to clear state after a restart or replan.
    fn pending_dwells(&self) -> Vec<String> {
        Vec::new()
    }
}
