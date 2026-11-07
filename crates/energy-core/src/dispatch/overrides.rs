//! The per-pack override set in force for one tick.

use crate::dispatch::ManualOverride;

/// Every override that applies this tick.
///
/// Later entries win: the engine builds maintenance overrides first and the operator's own above
/// them, so a manual command is never masked by the schedule.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    entries: Vec<ManualOverride>,
}

impl Overrides {
    /// An empty set.
    pub fn none() -> Self {
        Self::default()
    }

    /// A set from the given entries.
    pub fn new(entries: Vec<ManualOverride>) -> Self {
        Self { entries }
    }

    /// The override in force for one pack at `now`, if any.
    pub fn active(&self, pack_id: &str, now: i64) -> Option<&ManualOverride> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.pack_id == pack_id && entry.active(now))
    }

    /// The packs with an override in force.
    pub fn active_ids(&self, now: i64) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.active(now))
            .map(|entry| entry.pack_id.as_str())
            .collect()
    }

    /// How many entries the set holds, expired or not.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
