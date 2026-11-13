//! Hourly p50/p10/p90 bands for one component, from a learned profile or the fallback shape.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use chrono_tz::Tz;

use super::profile::LoadProfile;
use super::shape::{DEFAULT_SHAPE, FALLBACK_P10_RATIO, FALLBACK_P90_RATIO};
use super::temperature_fit::MIN_TEMPERATURE_R2;
use crate::stats::fsum;
use crate::time::weekday_hour_bucket;

/// Three hourly series covering the same hours, for one component of the forecast.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HourlyBands {
    /// Median (p50) kW per UTC hour.
    pub band: BTreeMap<DateTime<Utc>, f64>,
    /// p10 kW per UTC hour.
    pub low: BTreeMap<DateTime<Utc>, f64>,
    /// p90 kW per UTC hour.
    pub high: BTreeMap<DateTime<Utc>, f64>,
}

impl HourlyBands {
    /// Add another component's bands hour by hour, which is how the base and each device combine.
    pub fn add(&mut self, other: &HourlyBands) {
        for (key, value) in &other.band {
            *self.band.entry(*key).or_insert(0.0) += *value;
        }
        for (key, value) in &other.low {
            *self.low.entry(*key).or_insert(0.0) += *value;
        }
        for (key, value) in &other.high {
            *self.high.entry(*key).or_insert(0.0) += *value;
        }
    }
}

/// `(p50, p10, p90)` kW for every UTC hour in `[first_hour, last_hour]`, one component.
///
/// A device's own profile is a second profile above the house's, not a correction to it: every
/// component sees the same clock and the same temperature series. No floor is applied here; the
/// house base uses [`house_bands`] and the served total is floored in [`super::forecast`].
pub fn hourly_bands(
    profile: Option<&LoadProfile>,
    first_hour: DateTime<Utc>,
    last_hour: DateTime<Utc>,
    zone: Tz,
    base_kwh_per_day: f64,
    temperatures: &BTreeMap<DateTime<Utc>, f64>,
) -> HourlyBands {
    let Some(profile) = profile else {
        return fallback_bands(first_hour, last_hour, zone, base_kwh_per_day);
    };
    let mut bands = HourlyBands::default();
    let mut cursor = first_hour;
    while cursor <= last_hour {
        let index = weekday_hour_bucket(cursor, zone);
        let midpoint = profile.median(index) * profile.scale * profile.bias[index];
        let value = profile_value(profile, midpoint, cursor, temperatures);
        bands.band.insert(cursor, value);
        let bucket_median = profile.median(index);
        let (low, high) = if value > 0.0 && bucket_median > 0.0 {
            (
                value * profile.buckets[index].p10 / bucket_median,
                value * profile.buckets[index].p90 / bucket_median,
            )
        } else {
            (value * FALLBACK_P10_RATIO, value * FALLBACK_P90_RATIO)
        };
        bands.low.insert(cursor, low.min(value));
        bands.high.insert(cursor, high.max(value));
        cursor += Duration::hours(1);
    }
    bands
}

/// The house base component: the same bands, floored at the component's own standing load.
///
/// Only the house base is floored. A heat pump that cycles would be over-predicted every hour by
/// its night percentile, while the served *total* is floored separately at the house's own standing
/// load (see [`super::forecast`]).
pub fn house_bands(
    profile: Option<&LoadProfile>,
    first_hour: DateTime<Utc>,
    last_hour: DateTime<Utc>,
    zone: Tz,
    base_kwh_per_day: f64,
    temperatures: &BTreeMap<DateTime<Utc>, f64>,
) -> HourlyBands {
    let mut bands = hourly_bands(
        profile,
        first_hour,
        last_hour,
        zone,
        base_kwh_per_day,
        temperatures,
    );
    let Some(profile) = profile else {
        return bands;
    };
    let floor = profile.base_kw;
    for (hour, value) in &mut bands.band {
        let p50 = value.max(floor);
        *value = p50;
        if let Some(low) = bands.low.get_mut(hour) {
            *low = low.max(floor).min(p50);
        }
        if let Some(high) = bands.high.get_mut(hour) {
            *high = high.max(p50);
        }
    }
    bands
}

/// Apply the level, the temperature term and the zero floor to one bucket's midpoint.
fn profile_value(
    profile: &LoadProfile,
    midpoint: f64,
    cursor: DateTime<Utc>,
    temperatures: &BTreeMap<DateTime<Utc>, f64>,
) -> f64 {
    let mut value = midpoint;
    if profile.fit.b != 0.0
        && profile.fit.r2 >= MIN_TEMPERATURE_R2
        && let (Some(t_mean), Some(temperature)) = (profile.fit.t_mean, temperatures.get(&cursor))
    {
        value += profile.fit.b * (*temperature - t_mean);
    }
    value.max(0.0)
}

fn fallback_bands(
    first_hour: DateTime<Utc>,
    last_hour: DateTime<Utc>,
    zone: Tz,
    base_kwh_per_day: f64,
) -> HourlyBands {
    let total = fsum(DEFAULT_SHAPE.iter().copied());
    let mut bands = HourlyBands::default();
    let mut cursor = first_hour;
    while cursor <= last_hour {
        let hour = cursor.with_timezone(&zone).hour() as usize;
        let shape = DEFAULT_SHAPE.get(hour).copied().unwrap_or(0.0);
        let value = if total > 0.0 {
            (base_kwh_per_day * shape / total).max(0.0)
        } else {
            0.0
        };
        bands.band.insert(cursor, value);
        bands.low.insert(cursor, value * FALLBACK_P10_RATIO);
        bands.high.insert(cursor, value * FALLBACK_P90_RATIO);
        cursor += Duration::hours(1);
    }
    bands
}
