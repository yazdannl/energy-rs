//! The persisted per-pack cell-voltage latch, stored in the ledger's settings.

use std::sync::Arc;

use chrono::Utc;
use energy_battery::BatteryError;
use energy_battery::protection::{CellLatch, CellLatchStore};
use energy_ledger::{Ledger, LedgerError};

/// A [`CellLatchStore`] over the ledger's key/value settings.
///
/// One settings key per pack (`protection.latch.<serial>`) keeps the history table useful: a latch
/// change is a settings change, so the ledger's as-of read can answer "which pack was blocked at
/// 03:00" without a new table. A load failure is returned, never swallowed: booting with a latch
/// dropped is how a deep-discharged pack gets discharged again.
pub struct LedgerLatchStore {
    ledger: Arc<Ledger>,
}

impl LedgerLatchStore {
    /// Prefix of the per-pack settings keys.
    pub const KEY_PREFIX: &'static str = "protection.latch.";

    /// A store over the given ledger.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }

    /// The settings key for one pack.
    pub fn key(pack_id: &str) -> String {
        format!("{}{pack_id}", Self::KEY_PREFIX)
    }
}

impl CellLatchStore for LedgerLatchStore {
    fn load(&self, pack_id: &str) -> Result<Option<CellLatch>, BatteryError> {
        let raw = self
            .ledger
            .settings()
            .map_err(|error| store(&error))?
            .get(&Self::key(pack_id))
            .map_err(|error| store(&error))?;
        Ok(raw.and_then(|value| serde_json::from_str(&value).ok()))
    }

    fn save(&self, latch: &CellLatch) -> Result<(), BatteryError> {
        let json = serde_json::to_string(latch).map_err(|error| BatteryError::Invalid {
            reason: format!("cell latch serialisation failed: {error}"),
        })?;
        self.ledger
            .settings()
            .map_err(|error| store(&error))?
            .set(&Self::key(&latch.pack_id), &json, Utc::now().timestamp())
            .map_err(|error| store(&error))?;
        Ok(())
    }
}

fn store(error: &LedgerError) -> BatteryError {
    BatteryError::Invalid {
        reason: format!("cell latch store: {error}"),
    }
}
