//! What one evaluation decided for one pack.

use crate::protection::cell_latch::CellLatch;

/// The latch after one evaluation, plus whether this reading moved it.
///
/// `changed` is true when the latch was created or when `blocked`/`force_charge` moved, which is
/// exactly when the caller should persist and log a transition. A stale reading that keeps the
/// previous state reports `changed: false` — the state is the same, only the reason moved.
#[derive(Debug, Clone, PartialEq)]
pub struct CellDecision {
    /// The pack's latch after this reading.
    pub latch: CellLatch,
    /// Whether the latch was created or its block/force flags moved.
    pub changed: bool,
}

impl CellDecision {
    /// Whether the dispatcher must refuse discharge for this pack.
    pub const fn blocks_discharge(&self) -> bool {
        self.latch.blocks_discharge()
    }

    /// The forced charge power, if a charge is forced.
    pub const fn force_charge_w(&self) -> Option<u32> {
        if self.latch.force_charge {
            self.latch.force_charge_w
        } else {
            None
        }
    }
}
