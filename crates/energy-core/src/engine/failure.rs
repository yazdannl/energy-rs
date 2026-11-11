//! The tick failure boundary and orphan recovery.

use energy_ledger::EventLevel;

use crate::dispatch::TickOutcome;
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::safety::Reason;

impl Engine {
    /// The failure boundary of a tick.
    ///
    /// A tick that could not even read never reaches its progress marks, so repeated failures do
    /// trip the watchdog. Before that, neutralise the packs this process owns: the failed tick told
    /// us nothing about them, and keeping a setpoint we cannot justify is exactly what the latch
    /// exists to prevent.
    pub async fn tick_failed(&self, now: i64, error: &EngineError) {
        let _ = self.event(
            now,
            EventLevel::Error,
            "engine.tick_failed",
            error.to_string(),
        );
        let owned = self
            .lock()
            .map(|state| state.latch.pack_ids())
            .unwrap_or_default();
        if owned.is_empty() {
            return;
        }
        if let Err(error) = self.hold_all(now, Reason::TickFailed, Some(&owned)).await {
            let _ = self.event(
                now,
                EventLevel::Error,
                "hold.failed",
                format!("tick failure: {error}"),
            );
        }
    }

    /// Hold the packs we own that this tick could not command.
    ///
    /// A pack that disappears from the fleet read, or reports itself unavailable, is not a pack that
    /// keeps its last setpoint: it is hardware we drove and can no longer see, so it gets a
    /// best-effort hold and keeps its latch entry until a device confirms.
    /// Hold every pack we own, without a tick outcome to say which were commanded.
    ///
    /// Used when the tick itself failed: nothing was sent, so every latched pack is an orphan.
    pub(crate) async fn hold_orphans(&self, now: i64) {
        let owned = self
            .lock()
            .map(|state| state.latch.pack_ids())
            .unwrap_or_default();
        if owned.is_empty() {
            return;
        }
        if let Err(error) = self.hold_all(now, Reason::OrphanHold, Some(&owned)).await {
            tracing::warn!(%error, "orphan hold after a failed tick did not reach the devices");
        }
    }

    pub(crate) async fn reconcile(
        &self,
        now: i64,
        outcome: &TickOutcome,
    ) -> Result<(), EngineError> {
        let owned = self.lock()?.latch.pack_ids();
        let orphans: Vec<String> = owned
            .into_iter()
            .filter(|pack_id| !commanded(pack_id, outcome))
            .collect();
        if orphans.is_empty() {
            return Ok(());
        }
        if let Err(error) = self.hold_all(now, Reason::OrphanHold, Some(&orphans)).await {
            self.event(
                now,
                EventLevel::Error,
                "hold.failed",
                format!("orphan recovery: {error}"),
            )?;
        }
        Ok(())
    }
}

/// Whether this tick wrote the pack (or already fail-safed it), i.e. its latch row is current.
fn commanded(pack_id: &str, outcome: &TickOutcome) -> bool {
    outcome
        .report(pack_id)
        .is_some_and(|report| report.written || report.fail_safe)
}
