//! The latching low-cell-voltage guard.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::error::BatteryError;
use crate::protection::cell_config::CellConfig;
use crate::protection::cell_decision::CellDecision;
use crate::protection::cell_latch::CellLatch;
use crate::protection::cell_reading::CellReading;
use crate::protection::latch_reason::LatchReason;
use crate::protection::latch_store::CellLatchStore;

/// Per-pack cell-voltage protection, latched across restarts.
///
/// The state machine is deliberately tiny and total:
///
/// * `v < cutoff` → discharge blocked **and** an emergency charge at `min(max_charge, emergency)`;
/// * `cutoff <= v < resume` → discharge blocked;
/// * `v >= resume` → clear;
/// * a stale or missing reading → keep whatever the latch already said (the fail-safe direction);
/// * an unknown pack that has never been read is *not* invented into a latch: it stays idle until a
///   reading arrives, and a restored latch is kept verbatim.
///
/// All timestamps come from the caller, so the guard has no clock of its own and every transition is
/// reproducible in a test.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellVoltageGuard {
    latches: BTreeMap<String, CellLatch>,
}

impl CellVoltageGuard {
    /// An empty guard: every pack is unknown until it is evaluated or restored.
    pub fn new() -> Self {
        Self::default()
    }

    /// One pack's current latch, if it has ever been evaluated or restored.
    pub fn latch(&self, pack_id: &str) -> Option<&CellLatch> {
        self.latches.get(pack_id)
    }

    /// Every pack's current latch, in stable pack order.
    pub fn latches(&self) -> impl Iterator<Item = &CellLatch> {
        self.latches.values()
    }

    /// Evaluate one reading against one pack's thresholds.
    ///
    /// The returned `changed` is true when the latch was created or when its block/force flags moved;
    /// that is the signal to persist and log. A stale or missing reading never changes the flags, it
    /// only re-records why the previous state is being kept.
    pub fn evaluate(
        &mut self,
        pack_id: &str,
        reading: &CellReading,
        config: &CellConfig,
        now: DateTime<Utc>,
    ) -> CellDecision {
        let fresh = reading
            .voltage_v
            .filter(|_| reading.is_fresh(now, config.stale_after_s));
        let decision = match fresh {
            Some(voltage) => self.apply_voltage(pack_id, voltage, config, now),
            None => self.keep(pack_id, reading, now),
        };
        self.latches
            .insert(pack_id.to_owned(), decision.latch.clone());
        decision
    }

    /// Restore a pack's latch from the store; `true` when one was found.
    ///
    /// The persisted reason and `since` are kept: a restore is not a transition, and the operator
    /// needs to know why the pack is blocked, not when the add-on booted.
    pub fn restore(
        &mut self,
        store: &dyn CellLatchStore,
        pack_id: &str,
    ) -> Result<bool, BatteryError> {
        let Some(latch) = store.load(pack_id)? else {
            return Ok(false);
        };
        self.latches.insert(pack_id.to_owned(), latch);
        Ok(true)
    }

    /// Persist one pack's latch, if it has one.
    pub fn save(&self, store: &dyn CellLatchStore, pack_id: &str) -> Result<(), BatteryError> {
        if let Some(latch) = self.latches.get(pack_id) {
            store.save(latch)?;
        }
        Ok(())
    }

    /// Persist every known latch; returns how many were written.
    pub fn save_all(&self, store: &dyn CellLatchStore) -> Result<usize, BatteryError> {
        let mut saved = 0;
        for latch in self.latches.values() {
            store.save(latch)?;
            saved += 1;
        }
        Ok(saved)
    }

    /// A fresh reading: the state machine proper.
    fn apply_voltage(
        &self,
        pack_id: &str,
        voltage: f64,
        config: &CellConfig,
        now: DateTime<Utc>,
    ) -> CellDecision {
        let (blocked, force) = if voltage < config.cutoff_v {
            (true, true)
        } else if voltage < config.resume_v {
            (true, false)
        } else {
            (false, false)
        };
        let previous = self.latches.get(pack_id);
        let changed =
            previous.is_none_or(|latch| latch.blocked != blocked || latch.force_charge != force);
        let mut latch = previous
            .cloned()
            .unwrap_or_else(|| CellLatch::idle(pack_id, now));
        if changed {
            latch.since = now;
            latch.reason = reason_for(blocked, force);
            latch.reason_at = now;
        }
        latch.blocked = blocked;
        latch.force_charge = force;
        latch.force_charge_w = force.then(|| config.force_charge_w());
        latch.voltage_v = Some(voltage);
        latch.last_fresh_at = Some(now);
        CellDecision { latch, changed }
    }

    /// A stale or missing reading: keep the flags, re-record why.
    fn keep(&self, pack_id: &str, reading: &CellReading, now: DateTime<Utc>) -> CellDecision {
        let mut latch = self
            .latches
            .get(pack_id)
            .cloned()
            .unwrap_or_else(|| CellLatch::idle(pack_id, now));
        latch.reason = if reading.is_usable() {
            LatchReason::StaleReading
        } else {
            LatchReason::MissingReading
        };
        latch.reason_at = now;
        if let Some(voltage) = reading.voltage_v.filter(|value| value.is_finite()) {
            latch.voltage_v = Some(voltage);
        }
        CellDecision {
            latch,
            changed: false,
        }
    }
}

/// The reason that belongs to a block/force combination.
const fn reason_for(blocked: bool, force: bool) -> LatchReason {
    if force {
        LatchReason::BelowCutoff
    } else if blocked {
        LatchReason::BelowResume
    } else {
        LatchReason::AboveResume
    }
}
