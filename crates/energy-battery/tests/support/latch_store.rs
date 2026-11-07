//! An in-memory cell-latch store for the protection tests.

use std::collections::HashMap;
use std::sync::Mutex;

use energy_battery::{BatteryError, CellLatch, CellLatchStore};

/// A `CellLatchStore` backed by a map: what the engine persists, without a database.
#[derive(Debug, Default)]
pub struct MemoryLatchStore {
    rows: Mutex<HashMap<String, CellLatch>>,
}

impl MemoryLatchStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The saved latch for a pack, if any.
    pub fn latch(&self, pack_id: &str) -> Option<CellLatch> {
        self.rows.lock().unwrap().get(pack_id).cloned()
    }
}

impl CellLatchStore for MemoryLatchStore {
    fn load(&self, pack_id: &str) -> Result<Option<CellLatch>, BatteryError> {
        Ok(self.rows.lock().unwrap().get(pack_id).cloned())
    }

    fn save(&self, latch: &CellLatch) -> Result<(), BatteryError> {
        self.rows
            .lock()
            .unwrap()
            .insert(latch.pack_id.clone(), latch.clone());
        Ok(())
    }
}
