//! An in-memory [`LearningStore`], for tests, dry runs and the shadow engine.

use std::collections::BTreeMap;

use super::record::{CURRENT_SCHEMA_VERSION, LearningRecord};
use super::store::LearningStore;
use crate::ForecastError;

/// A `BTreeMap`-backed store. Deterministic iteration, no I/O.
#[derive(Debug, Clone, Default)]
pub struct MemoryLearningStore {
    records: BTreeMap<(String, String), LearningRecord>,
}

impl MemoryLearningStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl LearningStore for MemoryLearningStore {
    fn get(&self, kind: &str, key: &str) -> Option<LearningRecord> {
        self.records
            .get(&(kind.to_string(), key.to_string()))
            .cloned()
    }

    fn put(&mut self, record: LearningRecord) -> Result<(), ForecastError> {
        if record.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ForecastError::SchemaVersion {
                found: record.schema_version,
                expected: CURRENT_SCHEMA_VERSION,
            });
        }
        self.records
            .insert((record.kind.clone(), record.key.clone()), record);
        Ok(())
    }

    fn remove(&mut self, kind: &str, key: &str) -> bool {
        self.records
            .remove(&(kind.to_string(), key.to_string()))
            .is_some()
    }

    fn list(&self, kind: &str) -> Vec<LearningRecord> {
        self.records
            .values()
            .filter(|record| record.kind == kind)
            .cloned()
            .collect()
    }
}
