//! Everything one served load forecast needs.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::profile::LoadProfile;

/// The runtime inputs around a load forecast: the component profiles, weather, level and clock.
#[derive(Debug, Clone)]
pub struct LoadInputs<'a> {
    /// Stored profiles of the named devices, keyed by device key.
    pub device_profiles: &'a BTreeMap<String, LoadProfile>,
    /// Outdoor temperature by UTC hour, degrees C.
    pub temperatures: &'a BTreeMap<DateTime<Utc>, f64>,
    /// `load.base_kwh_per_day`, used only by the fallback shape.
    pub base_kwh_per_day: f64,
    /// The instant the forecast is built at.
    pub now: DateTime<Utc>,
    /// Local zone for bucket lookup.
    pub zone: Tz,
    /// Horizon in hours; `None` uses the default and the range is clamped.
    pub hours: Option<u32>,
    /// The house is away: serve the base floor plus only the devices that keep running.
    pub holiday: bool,
}

impl<'a> LoadInputs<'a> {
    /// Build the inputs; the caller passes the maps it already holds.
    pub fn new(
        device_profiles: &'a BTreeMap<String, LoadProfile>,
        temperatures: &'a BTreeMap<DateTime<Utc>, f64>,
        base_kwh_per_day: f64,
        now: DateTime<Utc>,
        zone: Tz,
        hours: Option<u32>,
    ) -> Self {
        Self {
            device_profiles,
            temperatures,
            base_kwh_per_day,
            now,
            zone,
            hours,
            holiday: false,
        }
    }

    /// The same inputs with the house away.
    #[must_use]
    pub fn with_holiday(mut self, holiday: bool) -> Self {
        self.holiday = holiday;
        self
    }
}
