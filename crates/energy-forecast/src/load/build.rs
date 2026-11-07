//! Learning the profile: buckets, level, daily bias and the temperature term.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

use super::bucket::{BIAS_MAX, BIAS_MIN, Bucket, EWMA_ALPHA, SCALE_MAX, SCALE_MIN};
use super::profile::LoadProfile;
use super::quantile::quantile;
use super::sample::LoadSample;
use super::shape::fill_assumed_shape;
use super::temperature_fit::fit_temperature;
use crate::stats::{mean, round_digits};
use crate::time::{iso_seconds, local_date, weekday_hour_bucket};

/// Days of history the profile is learned from.
pub const WINDOW_DAYS: u32 = 56;

/// Learn the 48-bucket median profile, level scale, EWMA bias and temperature slope.
///
/// `assumed_daytime_shape` fills buckets outside the observation window from the default shape and
/// must be decided here rather than by the caller, because the level and the bias both depend on the
/// medians it produces.
pub fn build_profile(
    samples: &[LoadSample],
    temperatures: &BTreeMap<DateTime<Utc>, f64>,
    now: DateTime<Utc>,
    zone: Tz,
    window_days: u32,
    assumed_daytime_shape: bool,
) -> LoadProfile {
    let usable = usable_samples(samples, now - Duration::days(i64::from(window_days)));
    let (mut medians, mut p10s, mut p90s) = bucket_stats(&usable, zone);
    if assumed_daytime_shape {
        fill_assumed_shape(&mut medians, &mut p10s, &mut p90s);
    }
    let buckets = assemble_buckets(&usable, zone, &medians, &p10s, &p90s);
    let profile_mean = weighted_mean(&medians);
    let recent_cutoff = now - Duration::days(7);
    let (scale, recent_7d_mean) = level(&usable, profile_mean, recent_cutoff);
    // The store persists `round(x, 4)` and serving reads that payload back, so the returned profile
    // carries the rounded values too: a profile that differs from its own payload is a trap.
    let mut bias = daily_bias(&usable, recent_cutoff, &medians, zone);
    for value in &mut bias {
        *value = round_digits(*value, 4);
    }
    LoadProfile {
        buckets,
        bias,
        scale: round_digits(scale, 4),
        profile_mean: round_digits(profile_mean, 4),
        recent_7d_mean: round_digits(recent_7d_mean, 4),
        fit: fit_temperature(&usable, temperatures),
        temperature_hours: temperature_hours(&usable, temperatures),
        samples: usable.len(),
        days: distinct_days(&usable, zone),
        assumed_daytime_shape,
        updated_at: iso_seconds(now),
        devices_subtracted: Vec::new(),
        devices_excluded: BTreeMap::new(),
        devices_dropped_hours: 0,
        devices_clamped_hours: 0,
        entity: None,
        kind: None,
        name: None,
    }
}

fn usable_samples(samples: &[LoadSample], cutoff: DateTime<Utc>) -> Vec<LoadSample> {
    samples
        .iter()
        .copied()
        .filter(|sample| sample.moment >= cutoff)
        .collect()
}

/// `(scale, recent_7d_mean)` from the last seven days of the window.
fn level(usable: &[LoadSample], profile_mean: f64, recent_cutoff: DateTime<Utc>) -> (f64, f64) {
    let recent: Vec<f64> = usable
        .iter()
        .filter(|sample| sample.moment >= recent_cutoff)
        .map(|sample| sample.load_kw)
        .collect();
    let recent_7d_mean = if recent.is_empty() {
        profile_mean
    } else {
        mean(&recent)
    };
    let scale = if profile_mean > 0.0 {
        (recent_7d_mean / profile_mean).clamp(SCALE_MIN, SCALE_MAX)
    } else {
        1.0
    };
    (scale, recent_7d_mean)
}

fn bucket_stats(samples: &[LoadSample], zone: Tz) -> ([f64; 48], [f64; 48], [f64; 48]) {
    let mut values: Vec<Vec<f64>> = vec![Vec::new(); 48];
    for sample in samples {
        if let Some(bucket) = values.get_mut(weekday_hour_bucket(sample.moment, zone)) {
            bucket.push(sample.load_kw);
        }
    }
    let mut medians = [0.0_f64; 48];
    let mut p10s = [0.0_f64; 48];
    let mut p90s = [0.0_f64; 48];
    for index in 0..48 {
        let bucket = &values[index];
        if !bucket.is_empty() {
            medians[index] = quantile(bucket, 0.5);
            p10s[index] = quantile(bucket, 0.1);
            p90s[index] = quantile(bucket, 0.9);
        }
    }
    (medians, p10s, p90s)
}

fn assemble_buckets(
    samples: &[LoadSample],
    zone: Tz,
    medians: &[f64; 48],
    p10s: &[f64; 48],
    p90s: &[f64; 48],
) -> [Bucket; 48] {
    let mut counts = [0u32; 48];
    for sample in samples {
        if let Some(count) = counts.get_mut(weekday_hour_bucket(sample.moment, zone)) {
            *count = count.saturating_add(1);
        }
    }
    let mut buckets = [Bucket::default(); 48];
    for index in 0..48 {
        buckets[index] = Bucket {
            median: round_digits(medians[index], 4),
            p10: round_digits(p10s[index], 4),
            p90: round_digits(p90s[index], 4),
            count: counts[index],
        };
    }
    buckets
}

fn weighted_mean(medians: &[f64; 48]) -> f64 {
    let weekday_mean = if medians[..24].iter().any(|value| *value != 0.0) {
        mean(&medians[..24])
    } else {
        0.0
    };
    let weekend_mean = if medians[24..].iter().any(|value| *value != 0.0) {
        mean(&medians[24..])
    } else {
        0.0
    };
    (5.0 * weekday_mean + 2.0 * weekend_mean) / 7.0
}

fn daily_bias(
    samples: &[LoadSample],
    recent_cutoff: DateTime<Utc>,
    medians: &[f64; 48],
    zone: Tz,
) -> [f64; 48] {
    let mut grouped: BTreeMap<(NaiveDate, usize), Vec<f64>> = BTreeMap::new();
    for sample in samples {
        if sample.moment < recent_cutoff {
            continue;
        }
        grouped
            .entry((
                local_date(sample.moment, zone),
                weekday_hour_bucket(sample.moment, zone),
            ))
            .or_default()
            .push(sample.load_kw);
    }
    let mut bias = [1.0_f64; 48];
    for ((_, bucket), values) in grouped {
        let median = medians[bucket];
        if median <= 0.0 {
            continue;
        }
        let ratio = (mean(&values) / median).clamp(BIAS_MIN, BIAS_MAX);
        bias[bucket] = (1.0 - EWMA_ALPHA) * bias[bucket] + EWMA_ALPHA * ratio;
    }
    bias
}

fn distinct_days(samples: &[LoadSample], zone: Tz) -> usize {
    let days: BTreeSet<NaiveDate> = samples
        .iter()
        .map(|sample| local_date(sample.moment, zone))
        .collect();
    days.len()
}

fn temperature_hours(samples: &[LoadSample], temperatures: &BTreeMap<DateTime<Utc>, f64>) -> usize {
    let hours: BTreeSet<DateTime<Utc>> = samples
        .iter()
        .map(|sample| crate::time::hour_floor(sample.moment))
        .filter(|hour| temperatures.contains_key(hour))
        .collect();
    hours.len()
}
