//! The persisted per-pack learning profile, stored in the ledger's settings.

use std::sync::Arc;

use chrono::Utc;
use energy_battery::BatteryError;
use energy_battery::learning::{LearningEvent, LearningProfile, LearningStore};
use energy_ledger::{Ledger, LedgerError, PendingLearningEvent};

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
        let json = encode(profile)?;
        self.ledger
            .settings()
            .map_err(|error| store(&error))?
            .set(&Self::key(&profile.pack_id), &json, Utc::now().timestamp())
            .map_err(|error| store(&error))?;
        Ok(())
    }

    fn save_with_event(
        &self,
        profile: &LearningProfile,
        event: LearningEvent,
    ) -> Result<LearningEvent, BatteryError> {
        let json = encode(profile)?;
        let id = self
            .ledger
            .settings()
            .map_err(|error| store(&error))?
            .set_with_learning_event(
                &Self::key(&profile.pack_id),
                &json,
                Utc::now().timestamp(),
                &pending(&event),
            )
            .map_err(|error| store(&error))?;
        Ok(LearningEvent { id, ..event })
    }

    fn pending_events(&self) -> Result<Vec<LearningEvent>, BatteryError> {
        let pending = self
            .ledger
            .learning_outbox()
            .map_err(|error| store(&error))?
            .pending()
            .map_err(|error| store(&error))?;
        Ok(pending.iter().map(event).collect())
    }

    fn publish_event(&self, event: &LearningEvent) -> Result<(), BatteryError> {
        // A duplicate claim or a quarantined row is not a pass failure: the audit payload is
        // either already journalled or preserved for inspection.
        self.ledger
            .learning_outbox()
            .map_err(|error| store(&error))?
            .publish(&pending(event))
            .map(|_| ())
            .map_err(|error| store(&error))
    }
}

fn encode(profile: &LearningProfile) -> Result<String, BatteryError> {
    serde_json::to_string(profile).map_err(|error| BatteryError::Invalid {
        reason: format!("learning profile serialisation failed: {error}"),
    })
}

fn pending(event: &LearningEvent) -> PendingLearningEvent {
    PendingLearningEvent::new(
        event.id,
        event.pack_id.clone(),
        event.at,
        event.summary.clone(),
    )
}

fn event(pending: &PendingLearningEvent) -> LearningEvent {
    LearningEvent {
        id: pending.id,
        pack_id: pending.pack_id.clone(),
        at: pending.at,
        summary: pending.summary.clone(),
    }
}

fn store(error: &LedgerError) -> BatteryError {
    BatteryError::Invalid {
        reason: format!("learning store: {error}"),
    }
}
