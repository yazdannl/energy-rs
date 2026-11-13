//! The learned load profile, its persistence and the freshness rule the planner applies.
//!
//! One 48-bucket profile is built from the energy dashboard's hourly history and stored through the
//! forecast crate's [`LearningStore`](energy_forecast::persistence::LearningStore). The planner asks
//! for the profile at plan time; a profile older than [`LoadProfiles::MAX_AGE_S`] counts as absent,
//! so a stopped refresh cannot keep steering the battery with last month's house.
//!
//! Beside the base profile the same store holds the operator's [`LoadConfig`] (holiday mode, the
//! weather sources and the named loads) and one profile per named load. All three travel together:
//! a device is only added back above the base when the base that is being served was learned with
//! that device subtracted.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use energy_forecast::ForecastError;
use energy_forecast::load::{
    LoadConfig, LoadDevice, LoadProfile, LoadSample, WINDOW_DAYS, base_load_kw, build_profile,
    split_device_load,
};
use energy_forecast::persistence::{LearningRecord, LearningStore};
use energy_forecast::time::iso_seconds;
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
    /// Key of the operator's load configuration (holiday, weather, named loads).
    pub const CONFIG_KEY: &'static str = "config";
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

    /// The operator's load configuration; at home, no weather, no named loads when nothing is set.
    pub fn config(&self) -> LoadConfig {
        self.store
            .lock()
            .ok()
            .and_then(|store| store.get(Self::KIND, Self::CONFIG_KEY))
            .map(|record| LoadConfig::from_payload(&record.payload))
            .unwrap_or_default()
    }

    /// Store the configuration. The caller nudges the learner: nothing here re-learns.
    pub fn set_config(&self, config: &LoadConfig) -> Result<(), ForecastError> {
        let record = LearningRecord::new(
            Self::KIND,
            Self::CONFIG_KEY,
            iso_seconds(Utc::now()),
            config.to_payload(),
        );
        let mut store = self
            .store
            .lock()
            .map_err(|_| ForecastError::Payload("load profile store is poisoned".to_owned()))?;
        store.put(record)
    }

    /// One named load's stored profile, when it has one.
    pub fn device(&self, key: &str) -> Option<LoadProfile> {
        let record = self.store.lock().ok()?.get(Self::KIND, key)?;
        LoadProfile::from_payload(&record.payload).ok()
    }

    /// The stored profiles of the configured loads, keyed by device key.
    pub fn device_profiles(&self, devices: &[LoadDevice]) -> BTreeMap<String, LoadProfile> {
        let mut profiles = BTreeMap::new();
        for device in devices {
            if let Some(profile) = self.device(&device.key) {
                profiles.insert(device.key.clone(), profile);
            }
        }
        profiles
    }

    /// Learn the base profile and one profile per subtracted load, then persist all of them.
    ///
    /// The base is learned from the house **minus** every device whose meter covers the window, so
    /// the two can be added back at serving time without counting anything twice. A device the
    /// split refused stays inside the base and is recorded as excluded, with the reason, in the
    /// base payload itself.
    #[allow(clippy::too_many_arguments)] // the four series are the pass, not a context object
    pub fn learn(
        &self,
        house: &[LoadSample],
        temperatures: &BTreeMap<DateTime<Utc>, f64>,
        devices: &[LoadDevice],
        device_samples: &BTreeMap<String, BTreeMap<DateTime<Utc>, f64>>,
        now: DateTime<Utc>,
    ) -> Result<LoadProfile, ForecastError> {
        let split = split_device_load(house, devices, device_samples);
        let mut base = build_profile(&split.base, temperatures, now, TARIFF_TZ, WINDOW_DAYS, true);
        // The house's own standing load, learned before anything was subtracted: the served total
        // is floored at it, so a component that models low can never plan a darker house.
        base.house_base_kw = base_load_kw(house, now, TARIFF_TZ);
        base.devices_subtracted.clone_from(&split.subtracted);
        base.devices_excluded.clone_from(&split.skipped);
        base.devices_dropped_hours = split.dropped_hours;
        base.devices_clamped_hours = split.clamped_hours;
        let mut store = self
            .store
            .lock()
            .map_err(|_| ForecastError::Payload("load profile store is poisoned".to_owned()))?;
        store.put(LearningRecord::new(
            Self::KIND,
            Self::KEY,
            base.updated_at.clone(),
            base.to_payload(),
        ))?;
        for device in devices {
            if !split.subtracted.contains(&device.key) {
                continue;
            }
            let Some(series) = device_samples.get(&device.key) else {
                continue;
            };
            let samples: Vec<LoadSample> = series
                .iter()
                .map(|(moment, load_kw)| LoadSample {
                    moment: *moment,
                    load_kw: *load_kw,
                })
                .collect();
            let mut component =
                build_profile(&samples, temperatures, now, TARIFF_TZ, WINDOW_DAYS, true);
            component.entity = Some(device.entity.clone());
            component.kind = Some(device.kind.as_str().to_string());
            component.name = Some(device.name.clone());
            store.put(LearningRecord::new(
                Self::KIND,
                device.key.clone(),
                component.updated_at.clone(),
                component.to_payload(),
            ))?;
        }
        Ok(base)
    }
}
