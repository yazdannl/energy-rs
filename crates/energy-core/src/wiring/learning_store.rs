//! The persisted per-pack learning profile, stored in the ledger's settings.

use std::sync::Arc;

use chrono::Utc;
use energy_battery::BatteryError;
use energy_battery::learning::{LearningProfile, LearningStore};
use energy_ledger::{Ledger, LedgerError};

/// A [`LearningStore`] over the ledger's key/value settings.
///
/// The learner itself is pure, so the profile has to live somewhere the planner can read after a
/// restart. The ledger's settings table already has as-of history and a schema that does not belong
/// to this crate, which is exactly what a small per-pack JSON document needs.
pub struct LedgerLearningStore {
    ledger: Arc<Ledger>,
}

impl LedgerLearningStore {
    /// Prefix of the per-pack settings keys.
    pub const KEY_PREFIX: &'static str = "learning.profile.";

    /// A store over the given ledger.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }

    /// The settings key for one pack.
    pub fn key(pack_id: &str) -> String {
        format!("{}{pack_id}", Self::KEY_PREFIX)
    }
}

impl LearningStore for LedgerLearningStore {
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError> {
        let raw = self
            .ledger
            .settings()
            .map_err(|error| store(&error))?
            .get(&Self::key(pack_id))
            .map_err(|error| store(&error))?;
        Ok(raw.and_then(|value| serde_json::from_str(&value).ok()))
    }

    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError> {
        let json = serde_json::to_string(profile).map_err(|error| BatteryError::Invalid {
            reason: format!("learning profile serialisation failed: {error}"),
        })?;
        self.ledger
            .settings()
            .map_err(|error| store(&error))?
            .set(&Self::key(&profile.pack_id), &json, Utc::now().timestamp())
            .map_err(|error| store(&error))?;
        Ok(())
    }
}

fn store(error: &LedgerError) -> BatteryError {
    BatteryError::Invalid {
        reason: format!("learning store: {error}"),
    }
}
