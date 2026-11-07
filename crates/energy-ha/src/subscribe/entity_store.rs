//! Entity states plus compressed-diff application.

use std::collections::{BTreeMap, HashMap};

use crate::{discovery::PowerReading, state::EntityState};

use super::{EntityDiffEvent, EntityUpdate};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityStore {
    states: BTreeMap<String, EntityState>,
}

impl EntityStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_states(states: impl IntoIterator<Item = EntityState>) -> Self {
        let mut store = Self::default();
        for state in states {
            store.states.insert(state.entity_id.clone(), state);
        }
        store
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn get(&self, entity_id: &str) -> Option<&EntityState> {
        self.states.get(entity_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &EntityState)> {
        self.states.iter()
    }

    /// Power readings for every entity whose state is numeric and power-unit-bearing.
    pub fn readings(&self) -> HashMap<String, PowerReading> {
        self.states
            .iter()
            .filter_map(|(entity_id, state)| {
                PowerReading::from_state(state).map(|reading| (entity_id.clone(), reading))
            })
            .collect()
    }

    /// Apply one compressed event, returning full-state updates for downstream consumers.
    pub fn apply(&mut self, event: &EntityDiffEvent) -> Vec<EntityUpdate> {
        let mut updates = Vec::new();
        for (entity_id, compressed) in &event.a {
            let state = compressed.to_entity(entity_id);
            self.states.insert(entity_id.clone(), state.clone());
            updates.push(EntityUpdate::Added(state));
        }
        for (entity_id, diff) in &event.c {
            if let Some(state) = self.apply_change(entity_id, diff) {
                updates.push(EntityUpdate::Changed(state));
            }
        }
        for entity_id in &event.r {
            if self.states.remove(entity_id).is_some() {
                updates.push(EntityUpdate::Removed(entity_id.clone()));
            }
        }
        updates
    }

    fn apply_change(
        &mut self,
        entity_id: &str,
        diff: &super::CompressedDiff,
    ) -> Option<EntityState> {
        let mut state = self.states.get(entity_id).cloned()?;
        if let Some(plus) = &diff.plus {
            if let Some(value) = &plus.s {
                state.state = value.clone();
            }
            if let Some(attributes) = &plus.a {
                for (key, value) in attributes {
                    state.attributes.insert(key.clone(), value.clone());
                }
            }
            if let Some(last_changed) = plus.lc {
                state.last_changed = Some(last_changed);
                state.last_updated = Some(plus.lu.unwrap_or(last_changed));
            } else if let Some(last_updated) = plus.lu {
                state.last_updated = Some(last_updated);
            }
        }
        if let Some(removed) = &diff.minus {
            for key in removed {
                state.attributes.remove(key);
            }
        }
        self.states.insert(entity_id.to_owned(), state.clone());
        Some(state)
    }
}
