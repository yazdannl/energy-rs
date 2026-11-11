//! Readback: what the packs say they are doing, against what they were told.

use std::collections::BTreeMap;

use crate::dispatch::Readback;
use crate::engine::Engine;
use crate::engine::error::EngineError;
use energy_ledger::EventLevel;

impl Engine {
    /// Fold this tick's commands and measurements into the per-pack compliance streaks.
    ///
    /// A pack that ignores its command for a few ticks is not a transport error the fail-safe path
    /// can see — the call succeeded, the device just did something else. It is treated as a control
    /// fault: the pack is latched (it may be executing something we did not order), held, and left
    /// alone for the fault backoff, so the loop cannot spend the day commanding a device that is
    /// not listening.
    pub(crate) fn check_readback(
        &self,
        now: i64,
        outcome: &crate::dispatch::TickOutcome,
        view: &crate::dispatch::FleetView,
    ) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        let mut offenders = Vec::new();
        for report in &outcome.packs {
            let reading = view
                .packs
                .iter()
                .find(|pack| pack.pack_id() == report.pack_id);
            let entry = state.readback.entry(report.pack_id.clone()).or_default();
            *entry = entry.fold(report, reading, now);
            if entry.ignoring() {
                offenders.push((report.pack_id.clone(), report.intended_w, now));
            }
        }
        state.readback.retain(|pack_id, _| {
            outcome
                .packs
                .iter()
                .any(|report| &report.pack_id == pack_id)
        });
        drop(state);
        for (pack_id, intended_w, at) in offenders {
            self.dispatcher.faults().arm(&pack_id, at);
            if let Ok(mut state) = self.lock() {
                // Keep ownership: whatever the pack is doing, it is doing it because we drove it.
                if state.latch.record(&pack_id, intended_w, at) {
                    state.latch_dirty = true;
                }
            }
            self.event(
                at,
                EventLevel::Error,
                "dispatch.readback",
                format!("{pack_id}: commanded {intended_w:.0} W, the pack reports something else"),
            )?;
        }
        Ok(())
    }
}

/// The compliance streaks, keyed by pack id.
pub type ReadbackMap = BTreeMap<String, Readback>;
