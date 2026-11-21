//! Readback: what the packs say they are doing, against what they were told.

use std::collections::BTreeMap;

use crate::dispatch::{LONG_FAULT_BACKOFF_S, Readback};
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
            // A suppressed tick is not a clean tick: keep the streak so a no-flow fault stays
            // authoritative across the backoff, including maintenance overrides.
            let backed_off = self.dispatcher.faults().blocking(&report.pack_id, now);
            *entry = entry.fold(report, reading, now, backed_off);
            if let Some(offence) =
                new_offence(self.dispatcher.faults(), report, reading, entry, now)
            {
                offenders.push(offence);
            }
        }
        state.readback.retain(|pack_id, _| {
            outcome
                .packs
                .iter()
                .any(|report| &report.pack_id == pack_id)
        });
        drop(state);
        for (pack_id, intended_w, at, no_flow) in offenders {
            let cause = if no_flow {
                "the pack echoed its command but no AC power flowed"
            } else {
                "the pack ignored its command"
            };
            self.dispatcher
                .faults()
                .arm_for(&pack_id, at, cause, LONG_FAULT_BACKOFF_S);
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

/// An unchanged streak belongs to the episode that already armed the backoff. Re-arming it after
/// expiry would make a no-flow fault self-perpetuating without new evidence.
fn new_offence(
    faults: &crate::dispatch::FaultGuard,
    report: &crate::dispatch::PackReport,
    reading: Option<&crate::dispatch::PackReading>,
    entry: &Readback,
    now: i64,
) -> Option<(String, f64, i64, bool)> {
    if !entry.ignoring() {
        return None;
    }
    let fresh = faults
        .episode_start(&report.pack_id)
        .is_none_or(|started| entry.since.is_some_and(|since| since > started));
    if !fresh {
        return None;
    }
    let no_flow = reading.is_some_and(|pack| {
        pack.flow
            .is_some_and(|flow| flow.failed_start(report.intended_w))
    });
    Some((report.pack_id.clone(), report.intended_w, now, no_flow))
}

/// The compliance streaks, keyed by pack id.
pub type ReadbackMap = BTreeMap<String, Readback>;
