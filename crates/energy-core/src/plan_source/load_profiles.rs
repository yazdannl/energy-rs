//! The learned load profile, its persistence and the freshness rule the planner applies.
//!
//! One 48-bucket profile is built from the energy dashboard's hourly history and stored through the
//! forecast crate's [`LearningStore`](energy_forecast::persistence::LearningStore). The planner asks
//! for the profile at plan time; a profile older than [`LoadProfiles::MAX_AGE_S`] counts as absent,
//! so a stopped refresh cannot keep steering the battery with last month's house.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use energy_forecast::ForecastError;
use energy_forecast::load::{LoadProfile, LoadSample, WINDOW_DAYS, build_profile};
use energy_forecast::persistence::{LearningRecord, LearningStore};
use energy_ledger::Ledger;
use energy_price::book::query::TARIFF_TZ;

use crate::wiring::forecast_store::LedgerForecastStore;

/// The learned base profile: one per house, not one per device.
pub struct LoadProfiles {
    store: Mutex<LedgerForecastStore>,
}

impl LoadProfiles {
    /// Record kind and key of the base profile.
    pub const KIND: &'static str = "load";
    /// Key of the base (whole-house) profile.
    pub const KEY: &'static str = "profile";
    /// A profile older than this is not used for planning, seconds (seven days).
    pub const MAX_AGE_S: i64 = 7 * 24 * 3600;
    /// Distinct local days a profile needs before the planner prefers it over the fallback.
    pub const MIN_DAYS: usize = 3;

    /// A store over the given ledger.
    pub fn new(ledger: Arc<Ledger>) -> Self {
        Self {
            store: Mutex::new(LedgerForecastStore::new(ledger)),
        }
    }

    /// The profile the planner may use at `now`, or `None` when there is none or it is stale.
    pub fn current(&self, now: i64) -> Option<LoadProfile> {
        let record = self.store.lock().ok()?.get(Self::KIND, Self::KEY)?;
        let profile = match LoadProfile::from_payload(&record.payload) {
            Ok(profile) => profile,
            Err(error) => {
                tracing::warn!(%error, "stored load profile cannot be decoded");
                return None;
            }
        };
        if profile.days < Self::MIN_DAYS {
            return None;
        }
        let updated = chrono::DateTime::parse_from_rfc3339(&profile.updated_at)
            .ok()
            .map(|stamp| stamp.timestamp())?;
        (now.saturating_sub(updated) <= Self::MAX_AGE_S).then_some(profile)
    }

    /// Learn a profile from `samples` and persist it.
    ///
    /// The daytime shape is assumed for buckets outside the observation window: the house always
    /// has a daytime load, and leaving those buckets at zero would plan a night that never existed.
    pub fn refresh(
        &self,
        samples: &[LoadSample],
        now: DateTime<Utc>,
    ) -> Result<LoadProfile, ForecastError> {
        let temperatures = std::collections::BTreeMap::new();
        let profile = build_profile(samples, &temperatures, now, TARIFF_TZ, WINDOW_DAYS, true);
        let record = LearningRecord::new(
            Self::KIND,
            Self::KEY,
            profile.updated_at.clone(),
            profile.to_payload(),
        );
        let mut store = self
            .store
            .lock()
            .map_err(|_| ForecastError::Payload("load profile store is poisoned".to_owned()))?;
        store.put(record)?;
        Ok(profile)
    }
}
