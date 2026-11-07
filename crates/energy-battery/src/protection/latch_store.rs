//! Persistence for cell-voltage latches.

use crate::error::BatteryError;
use crate::protection::cell_latch::CellLatch;

/// Where the per-pack cell-voltage latch is stored.
///
/// The latch must outlive the process, so the engine (which owns the database) implements this and
/// the guard stays pure. A load failure is returned: booting with a latch silently dropped is how a
/// deep-discharged pack gets discharged again.
pub trait CellLatchStore: Send + Sync {
    /// The saved latch for `pack_id`, or `None` when the pack was never evaluated.
    fn load(&self, pack_id: &str) -> Result<Option<CellLatch>, BatteryError>;

    /// Replace the latch (the latch carries its own pack id).
    fn save(&self, latch: &CellLatch) -> Result<(), BatteryError>;
}
