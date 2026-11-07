//! An in-memory learning store for the learner tests.

use std::collections::HashMap;
use std::sync::Mutex;

use energy_battery::{BatteryError, LearningProfile, LearningStore};

/// A `LearningStore` backed by a map: what the engine would persist, without a database.
#[derive(Debug, Default)]
pub struct MemoryLearningStore {
    rows: Mutex<HashMap<String, LearningProfile>>,
}

impl MemoryLearningStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The saved profile for a pack, if any.
    pub fn profile(&self, pack_id: &str) -> Option<LearningProfile> {
        self.rows.lock().unwrap().get(pack_id).cloned()
    }
}

impl LearningStore for MemoryLearningStore {
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError> {
        Ok(self.rows.lock().unwrap().get(pack_id).cloned())
    }

    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError> {
        self.rows
            .lock()
            .unwrap()
            .insert(profile.pack_id.clone(), profile.clone());
        Ok(())
    }
}
