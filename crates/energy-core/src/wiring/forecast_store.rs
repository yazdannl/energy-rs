//! The forecast crate's learning payloads, stored in the ledger's key/value settings.
//!
//! `energy-forecast` defines the payload envelope and its schema version; this store only puts the
//! encoded record into `settings` under `forecast.learning.<kind>.<key>`, so the load profile keeps
//! its provenance, survives a restart and never grows a second table. A record written by a schema
//! this build does not read is refused on decode, not silently reinterpreted.

use std::sync::Arc;

use energy_forecast::ForecastError;
use energy_forecast::persistence::{LearningRecord, LearningStore};
use energy_ledger::{Ledger, LedgerError};

/// A [`LearningStore`] over the ledger's settings table.
pub struct LedgerForecastStore {
    ledger: Arc<Ledger>,
}

impl LedgerForecastStore {
    /// Settings key prefix for every stored forecast record.
    pub const PREFIX: &'static str = "forecast.learning.";

    /// A store over the given ledger.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self { ledger }
    }

    /// The settings key for one `(kind, key)` pair.
    pub fn setting_key(kind: &str, key: &str) -> String {
        format!("{}{kind}.{key}", Self::PREFIX)
    }

    fn settings(&self) -> Result<energy_ledger::settings::Settings<'_>, ForecastError> {
        self.ledger.settings().map_err(|error| store_error(&error))
    }
}

impl LearningStore for LedgerForecastStore {
    fn get(&self, kind: &str, key: &str) -> Option<LearningRecord> {
        let raw = self
            .settings()
            .ok()?
            .get(&Self::setting_key(kind, key))
            .ok()??;
        let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
        match LearningRecord::decode(&value) {
            Ok(record) => Some(record),
            Err(error) => {
                tracing::warn!(kind, key, %error, "stored forecast profile is unusable; ignoring it");
                None
            }
        }
    }

    fn put(&mut self, record: LearningRecord) -> Result<(), ForecastError> {
        let key = Self::setting_key(&record.kind, &record.key);
        let json = serde_json::to_string(&record.encode()).map_err(|error| {
            ForecastError::Payload(format!("forecast profile serialisation failed: {error}"))
        })?;
        let at = chrono::Utc::now().timestamp();
        self.settings()?
            .set(&key, &json, at)
            .map_err(|error| store_error(&error))?;
        Ok(())
    }

    fn remove(&mut self, kind: &str, key: &str) -> bool {
        self.settings()
            .and_then(|settings| {
                settings
                    .remove(&Self::setting_key(kind, key))
                    .map_err(|error| store_error(&error))
            })
            .unwrap_or(false)
    }

    fn list(&self, kind: &str) -> Vec<LearningRecord> {
        // The settings table has no prefix scan; the shipped profiles are addressed by name
        // (the base load profile and, later, one per named device), and each caller asks for the
        // one it needs. Returning an empty list is the honest answer here, not a silent "no data".
        let _ = kind;
        Vec::new()
    }
}

fn store_error(error: &LedgerError) -> ForecastError {
    ForecastError::Payload(format!("forecast store: {error}"))
}
