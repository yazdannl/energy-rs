//! The live-write latch: what we left running, persisted across restarts.

use std::collections::BTreeMap;

use energy_ledger::Ledger;
use serde::{Deserialize, Serialize};

use crate::engine::error::EngineError;
use crate::engine::latched_write::LatchedWrite;
use crate::engine::{LATCH_TOLERANCE_W, LIVE_LATCH_KEY};

/// The setpoints this process has written and not yet seen neutralised.
///
/// The rule is one-sided on purpose: a *blocked* or dry-run decision never latches (nothing was
/// written), and only an explicit, verified hold clears an entry. A momentary 0 W measurement never
/// clears it, because the device may still be executing the last command (survey §7 trap 3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveLatch {
    entries: BTreeMap<String, LatchedWrite>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    batteries: Vec<StoredEntry>,
    updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry {
    pack_id: String,
    power_w: f64,
    since: i64,
}

impl LiveLatch {
    /// An empty latch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the latch from the ledger, every entry marked as a boot setpoint.
    ///
    /// A value that does not parse, or an entry that cannot name a pack, is a recovery error, never
    /// an empty latch: an empty latch would silently forget the packs a previous run left live and
    /// the next boot would neutralise nothing. The stored value is left untouched so an operator (or
    /// `doctor`) can still read it, and `auto` control stays refused until it is reconciled.
    pub fn load(ledger: &Ledger) -> Result<Self, EngineError> {
        let Some(raw) = ledger.settings()?.get(LIVE_LATCH_KEY)? else {
            return Ok(Self::new());
        };
        let stored: Stored = serde_json::from_str(&raw)
            .map_err(|_| EngineError::LatchCorrupt { bytes: raw.len() })?;
        let mut entries = BTreeMap::new();
        for entry in stored.batteries {
            if entry.pack_id.is_empty() || !entry.power_w.is_finite() {
                return Err(EngineError::LatchCorrupt { bytes: raw.len() });
            }
            entries.insert(
                entry.pack_id.clone(),
                LatchedWrite {
                    pack_id: entry.pack_id,
                    power_w: entry.power_w,
                    since: entry.since,
                    boot: true,
                },
            );
        }
        Ok(Self { entries })
    }

    /// Persist the latch (an empty latch writes an empty list, never deletes the key).
    ///
    /// A serialisation failure is an error: writing an empty list instead would erase the packs a
    /// previous run left live.
    pub fn save(&self, ledger: &Ledger, now: i64) -> Result<(), EngineError> {
        let stored = Stored {
            batteries: self
                .entries
                .values()
                .map(|entry| StoredEntry {
                    pack_id: entry.pack_id.clone(),
                    power_w: entry.power_w,
                    since: entry.since,
                })
                .collect(),
            updated_at: now,
        };
        let json = serde_json::to_string(&stored).map_err(|error| EngineError::LatchWrite {
            reason: error.to_string(),
        })?;
        ledger.settings()?.set(LIVE_LATCH_KEY, &json, now)?;
        Ok(())
    }

    /// Record a real non-zero write; returns whether the persisted value changed.
    pub fn record(&mut self, pack_id: &str, power_w: f64, now: i64) -> bool {
        if power_w.abs() <= LATCH_TOLERANCE_W {
            return false;
        }
        let Some(entry) = self.entries.get_mut(pack_id) else {
            self.entries.insert(
                pack_id.to_owned(),
                LatchedWrite {
                    pack_id: pack_id.to_owned(),
                    power_w,
                    since: now,
                    boot: false,
                },
            );
            return true;
        };
        let changed = entry.boot || (entry.power_w - power_w).abs() > LATCH_TOLERANCE_W;
        entry.power_w = power_w;
        entry.since = now;
        entry.boot = false;
        changed
    }

    /// Forget a pack after a verified hold.
    pub fn clear(&mut self, pack_id: &str) -> bool {
        self.entries.remove(pack_id).is_some()
    }

    /// The setpoint we left on one pack.
    pub fn get(&self, pack_id: &str) -> Option<&LatchedWrite> {
        self.entries.get(pack_id)
    }

    /// The setpoints restored from a previous process.
    pub fn boot_entries(&self) -> impl Iterator<Item = &LatchedWrite> {
        self.entries.values().filter(|entry| entry.boot)
    }

    /// The packs still latched, in stable order.
    pub fn pack_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// The latch size.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing of ours is live.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
