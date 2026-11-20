//! The ledger-backed maintenance history.

use std::sync::Arc;

use energy_ledger::{Ledger, LedgerError};

use crate::maintenance::FullChargeHistory;

/// Per-pack balancing history stored on the pack row.
#[derive(Clone)]
pub struct LedgerFullChargeHistory {
    ledger: Arc<Ledger>,
}

impl LedgerFullChargeHistory {
    /// History over the shared ledger.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }
}

impl FullChargeHistory for LedgerFullChargeHistory {
    fn last_full_at(&self, pack_id: &str) -> Option<i64> {
        self.ledger
            .packs()
            .ok()?
            .by_sn(pack_id)
            .ok()?
            .and_then(|pack| pack.last_full_charge_at)
    }

    fn dwell_started_at(&self, pack_id: &str) -> Option<i64> {
        self.ledger
            .packs()
            .ok()?
            .by_sn(pack_id)
            .ok()?
            .and_then(|pack| pack.dwell_started_at)
    }

    fn begin_dwell(&self, pack_id: &str, at: i64) -> Result<(), LedgerError> {
        self.ledger.packs()?.begin_dwell(pack_id, at)
    }

    fn finish_dwell(&self, pack_id: &str, at: i64) -> Result<(), LedgerError> {
        self.ledger.packs()?.finish_dwell(pack_id, at)
    }

    fn abort_dwell(&self, pack_id: &str) -> Result<(), LedgerError> {
        self.ledger.packs()?.abort_dwell(pack_id)
    }
}
