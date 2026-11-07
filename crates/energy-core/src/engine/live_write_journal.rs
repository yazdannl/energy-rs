//! The live-write latch as the write-intent journal.

use std::sync::{Arc, Mutex};

use energy_ledger::Ledger;

use crate::dispatch::SinkError;
use crate::engine::state::EngineState;
use crate::engine::write_intent::WriteIntent;

/// Persist ownership in the live-write latch before a command leaves for a device.
///
/// The engine's `record` step runs after the whole tick, which leaves a window in which a crash
/// loses the pack's ownership entirely: the device keeps executing a setpoint nobody knows about,
/// and the next boot has nothing to neutralise. Claiming the setpoint here closes that window: the
/// latch row is durable before the transport call, and an unverified or failed write leaves it in
/// place (only a verified hold clears it).
pub(crate) struct LiveWriteJournal {
    state: Arc<Mutex<EngineState>>,
    ledger: Arc<Ledger>,
}

impl LiveWriteJournal {
    /// A journal over the engine's latch and the ledger it persists to.
    pub(crate) fn new(state: Arc<Mutex<EngineState>>, ledger: Arc<Ledger>) -> Self {
        Self { state, ledger }
    }
}

impl WriteIntent for LiveWriteJournal {
    fn claim(&self, pack_id: &str, watts: f64, now: i64) -> Result<(), SinkError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| refused(pack_id, "engine state is poisoned"))?;
        if !state.latch.record(pack_id, watts, now) {
            return Ok(());
        }
        state
            .latch
            .save(&self.ledger, now)
            .map_err(|error| refused(pack_id, error.to_string()))?;
        state.latch_dirty = false;
        Ok(())
    }
}

fn refused(pack_id: &str, reason: impl Into<String>) -> SinkError {
    SinkError::NotOwned {
        pack_id: pack_id.to_owned(),
        reason: reason.into(),
    }
}
