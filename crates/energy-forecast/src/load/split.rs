//! Subtracting device meters from the house total before the base profile is learned.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use super::device::LoadDevice;
use super::sample::LoadSample;

/// The share of the house's own hours a device meter must cover before its series is subtracted.
pub const DEVICE_MIN_COVERAGE: f64 = 0.5;
/// At least this many usable hourly readings in the learning window.
pub const DEVICE_MIN_SAMPLES: usize = 48;

/// The house total with every measurable device removed, plus why the rest stayed in it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSplit {
    /// What the base profile is learned from.
    pub base: Vec<LoadSample>,
    /// Device keys that were removed (the same keys the forecast may add back).
    pub subtracted: Vec<String>,
    /// Device key to the reason it was **not** removed, so it stays inside the base.
    pub skipped: BTreeMap<String, String>,
    /// House hours left out because a subtracted device had no reading for them.
    pub dropped_hours: u32,
    /// House hours where the device meters claimed more than the total, so the hour was dropped.
    pub clamped_hours: u32,
}

/// Subtract every measurable device's meter from the house total.
///
/// A missing device reading is unknown, not zero: reading it as zero would leave that device's draw
/// in the base and then add the device's own profile on top of it. A device whose meter covers too
/// little of the window is refused outright, and a set that cannot leave a usable intersection is
/// refused whole.
pub fn split_device_load(
    samples: &[LoadSample],
    devices: &[LoadDevice],
    device_samples: &BTreeMap<String, BTreeMap<DateTime<Utc>, f64>>,
) -> DeviceSplit {
    let mut skipped = BTreeMap::new();
    if samples.is_empty() {
        for device in devices {
            skipped.insert(
                device.key.clone(),
                "there is no house history to subtract it from".to_string(),
            );
        }
        return DeviceSplit::untouched(Vec::new(), skipped);
    }
    let house_stamps: BTreeSet<DateTime<Utc>> =
        samples.iter().map(|sample| sample.moment).collect();
    let eligible = select_eligible(&house_stamps, devices, device_samples, &mut skipped);
    if eligible.is_empty() {
        return DeviceSplit::untouched(samples.to_vec(), skipped);
    }
    let joint = joint_hours(&house_stamps, &eligible, device_samples);
    if (joint.len() as f64) < DEVICE_MIN_COVERAGE * house_stamps.len() as f64 {
        let reason = format!(
            "together with the other device meters it leaves only {} of the house's {} hours to learn the base profile from",
            joint.len(),
            house_stamps.len()
        );
        for device in &eligible {
            skipped.insert(device.key.clone(), reason.clone());
        }
        return DeviceSplit::untouched(samples.to_vec(), skipped);
    }
    let (base, dropped_hours, clamped_hours) = subtract(samples, &eligible, device_samples);
    // Dropping contradiction hours must not leave too little to learn from: a base profile built
    // from a quarter of the window is a different house, and adding the devices back on top of it
    // would be worse than leaving them inside the base.
    if (base.len() as f64) < DEVICE_MIN_COVERAGE * samples.len() as f64 {
        let reason = format!(
            "with the other meters subtracted only {} of the house's {} hours are left to learn the base profile from (the meters contradict the house total in the rest)",
            base.len(),
            samples.len()
        );
        for device in &eligible {
            skipped.insert(device.key.clone(), reason.clone());
        }
        return DeviceSplit::untouched(samples.to_vec(), skipped);
    }
    DeviceSplit {
        base,
        subtracted: eligible.iter().map(|device| device.key.clone()).collect(),
        skipped,
        dropped_hours,
        clamped_hours,
    }
}

impl DeviceSplit {
    /// The house total unchanged: nothing could be subtracted.
    fn untouched(base: Vec<LoadSample>, skipped: BTreeMap<String, String>) -> Self {
        Self {
            base,
            subtracted: Vec::new(),
            skipped,
            dropped_hours: 0,
            clamped_hours: 0,
        }
    }
}

/// Devices whose meters cover enough of the house's window to be subtracted; the rest are skipped
/// with a reason.
fn select_eligible<'a>(
    house_stamps: &BTreeSet<DateTime<Utc>>,
    devices: &'a [LoadDevice],
    device_samples: &BTreeMap<String, BTreeMap<DateTime<Utc>, f64>>,
    skipped: &mut BTreeMap<String, String>,
) -> Vec<&'a LoadDevice> {
    let mut eligible = Vec::new();
    for device in devices {
        let Some(series) = device_samples.get(&device.key) else {
            skipped.insert(
                device.key.clone(),
                "its meter returned no usable hours in the learning window".to_string(),
            );
            continue;
        };
        if series.len() < DEVICE_MIN_SAMPLES {
            skipped.insert(
                device.key.clone(),
                format!(
                    "its meter returned only {} usable hours in the learning window (fewer than {DEVICE_MIN_SAMPLES})",
                    series.len()
                ),
            );
            continue;
        }
        let overlap = series
            .keys()
            .filter(|stamp| house_stamps.contains(stamp))
            .count();
        let coverage = overlap as f64 / house_stamps.len() as f64;
        if coverage < DEVICE_MIN_COVERAGE {
            skipped.insert(
                device.key.clone(),
                format!(
                    "its meter overlaps only {overlap} of the house's {} hours ({:.0}%)",
                    house_stamps.len(),
                    coverage * 100.0
                ),
            );
            continue;
        }
        eligible.push(device);
    }
    eligible
}

/// Hours every eligible meter covers.
fn joint_hours(
    house_stamps: &BTreeSet<DateTime<Utc>>,
    eligible: &[&LoadDevice],
    device_samples: &BTreeMap<String, BTreeMap<DateTime<Utc>, f64>>,
) -> BTreeSet<DateTime<Utc>> {
    let mut joint = house_stamps.clone();
    for device in eligible {
        if let Some(series) = device_samples.get(&device.key) {
            let stamps: BTreeSet<DateTime<Utc>> = series.keys().copied().collect();
            joint = joint.intersection(&stamps).copied().collect();
        }
    }
    joint
}

/// `(base, dropped_hours, clamped_hours)`: the house total minus the eligible meters.
fn subtract(
    samples: &[LoadSample],
    eligible: &[&LoadDevice],
    device_samples: &BTreeMap<String, BTreeMap<DateTime<Utc>, f64>>,
) -> (Vec<LoadSample>, u32, u32) {
    let mut sorted = samples.to_vec();
    sorted.sort_by_key(|sample| sample.moment);
    let mut base = Vec::with_capacity(sorted.len());
    let mut dropped = 0u32;
    let mut clamped = 0u32;
    for sample in &sorted {
        let mut removed = 0.0;
        let mut complete = true;
        for device in eligible {
            let series = device_samples.get(&device.key);
            if let Some(value) = series.and_then(|values| values.get(&sample.moment)) {
                removed += *value;
            } else {
                complete = false;
                break;
            }
        }
        if !complete {
            dropped = dropped.saturating_add(1);
            continue;
        }
        let value = sample.load_kw - removed;
        if value < 0.0 {
            // The meters claim more than the house used, which cannot be true: the hour is a
            // measurement contradiction, unknown rather than zero. Zero-filling it teaches the
            // base profile a house that drew nothing (and drags its floor to zero), so drop it.
            clamped = clamped.saturating_add(1);
            continue;
        }
        base.push(LoadSample {
            moment: sample.moment,
            load_kw: value,
        });
    }
    (base, dropped, clamped)
}
