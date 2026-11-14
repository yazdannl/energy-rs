//! Serving: hourly bands summed, then linearly interpolated to 15-minute slots.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};

use super::bands::{HourlyBands, hourly_bands, house_bands};
use super::confidence::Confidence;
use super::device::LoadDevice;
use super::inputs::LoadInputs;
use super::plan::{DevicePlan, device_plan};
use super::profile::LoadProfile;
use super::shape::{FALLBACK_P10_RATIO, FALLBACK_P90_RATIO};
use super::slot::LoadSlot;
use super::source::LoadSource;
use crate::time::{SLOT_MINUTES, hour_floor, quarter_floor, slot_range};

/// Default forecast horizon, hours.
pub const DEFAULT_HORIZON_HOURS: u32 = 36;
/// Shortest served horizon, hours.
pub const MIN_HORIZON_HOURS: u32 = 12;
/// Longest served horizon, hours.
pub const MAX_HORIZON_HOURS: u32 = 48;

/// A served load forecast: the slots, where they came from and the notes that explain them.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadForecast {
    /// Quarter-hour slots covering the horizon.
    pub slots: Vec<LoadSlot>,
    /// When the forecast was built.
    pub generated_at: DateTime<Utc>,
    /// `profile`, `fallback` or `fallback-empty`.
    pub source: LoadSource,
    /// Confidence class from the profile's evidence.
    pub confidence: Confidence,
    /// Human-readable notes: cold start and one line per device decision.
    pub notes: Vec<String>,
}

/// One hour-quantity linearly interpolated to a slot, flat at the last hour.
pub fn interpolated(
    values: &BTreeMap<DateTime<Utc>, f64>,
    hour_key: DateTime<Utc>,
    slot_time: DateTime<Utc>,
) -> f64 {
    let value = values.get(&hour_key).copied().unwrap_or(0.0);
    let next = values.get(&(hour_key + Duration::hours(1))).copied();
    match next {
        Some(next_value) => value + (next_value - value) * (f64::from(slot_time.minute()) / 60.0),
        None => value,
    }
}

/// Build the 15-minute load forecast from the learned profile (or the fallback shape).
///
/// The horizon is clamped to `12..=48` hours. The bands are summed component by component and then
/// interpolated, so a device profile adds exactly where the point forecast adds.
pub fn load_forecast(
    profile: Option<&LoadProfile>,
    devices: &[LoadDevice],
    inputs: &LoadInputs<'_>,
) -> LoadForecast {
    let horizon = inputs
        .hours
        .unwrap_or(DEFAULT_HORIZON_HOURS)
        .clamp(MIN_HORIZON_HOURS, MAX_HORIZON_HOURS);
    let start = quarter_floor(inputs.now);
    let grid = slot_range(
        start,
        start + Duration::hours(i64::from(horizon)),
        i64::from(SLOT_MINUTES),
    );
    let plan = device_plan(profile, devices, inputs.device_profiles, inputs.holiday);
    let total = summed_bands(profile, &plan, &grid, inputs);
    LoadForecast {
        slots: slots_from(&grid, &total),
        generated_at: inputs.now,
        source: LoadSource::classify(profile.is_some(), inputs.base_kwh_per_day),
        confidence: Confidence::from_profile(profile),
        notes: notes_for(profile, &plan, inputs),
    }
}

/// Sum every component's hourly bands over the hours the grid needs.
fn summed_bands(
    profile: Option<&LoadProfile>,
    plan: &DevicePlan,
    grid: &[DateTime<Utc>],
    inputs: &LoadInputs<'_>,
) -> HourlyBands {
    let (Some(start), Some(last)) = (grid.first().copied(), grid.last().copied()) else {
        return HourlyBands::default();
    };
    let first_hour = hour_floor(start) - Duration::hours(1);
    let last_hour = hour_floor(last) + Duration::hours(2);
    let mut total = HourlyBands::default();
    let house_floor = profile.map_or(0.0, |base| base.house_base_kw);
    let mut components: Vec<Option<&LoadProfile>> = Vec::new();
    match (inputs.holiday, profile) {
        // Away: the learned shape is the house at home, so it is replaced by the floor. The
        // devices that keep running are still added below.
        (true, Some(base)) => total = flat_bands(base.base_kw, first_hour, last_hour),
        _ => components.push(profile),
    }
    for (_, component) in &plan.included {
        components.push(Some(component));
    }
    for (index, component) in components.into_iter().enumerate() {
        // Only the first component is the house base, and only at home; the rest are named loads.
        let bands = if index == 0 && !inputs.holiday {
            house_bands(component, first_hour, last_hour, inputs)
        } else {
            hourly_bands(component, first_hour, last_hour, inputs)
        };
        total.add(&bands);
    }
    // The house never draws less than its standing load, whatever the components say. Away, the
    // floor *is* the served shape, so flooring it again would count the house twice.
    if !inputs.holiday {
        for value in total.band.values_mut() {
            *value = value.max(house_floor);
        }
        for value in total.low.values_mut() {
            *value = value.max(house_floor);
        }
    }
    total
}

/// Every hour at the base floor, with the fallback band width: the away shape.
fn flat_bands(floor: f64, first_hour: DateTime<Utc>, last_hour: DateTime<Utc>) -> HourlyBands {
    let mut bands = HourlyBands::default();
    let value = floor.max(0.0);
    let mut cursor = first_hour;
    while cursor <= last_hour {
        bands.band.insert(cursor, value);
        bands.low.insert(cursor, value * FALLBACK_P10_RATIO);
        bands.high.insert(cursor, value * FALLBACK_P90_RATIO);
        cursor += Duration::hours(1);
    }
    bands
}

/// Interpolate each hour-quantity onto the quarter-hour grid.
fn slots_from(grid: &[DateTime<Utc>], total: &HourlyBands) -> Vec<LoadSlot> {
    grid.iter()
        .map(|slot| {
            let hour_key = hour_floor(*slot);
            LoadSlot {
                t: *slot,
                load_kw: interpolated(&total.band, hour_key, *slot).max(0.0),
                p10_kw: interpolated(&total.low, hour_key, *slot).max(0.0),
                p90_kw: interpolated(&total.high, hour_key, *slot).max(0.0),
            }
        })
        .collect()
}

fn notes_for(
    profile: Option<&LoadProfile>,
    plan: &DevicePlan,
    inputs: &LoadInputs<'_>,
) -> Vec<String> {
    let mut notes = Vec::new();
    if profile.is_none() {
        notes.push("cold start: no learned profile yet".to_string());
    }
    if inputs.holiday {
        let floor = profile.map_or(0.0, |base| base.base_kw);
        notes.push(format!(
            "holiday mode: the house shape is replaced by the base floor ({:.0} W) plus {} load(s) that keep running",
            floor * 1000.0,
            plan.included.len()
        ));
    }
    notes.extend(plan.notes.iter().cloned());
    if !plan.included.is_empty() {
        let names: Vec<String> = plan
            .included
            .iter()
            .map(|(device, _)| device.label())
            .collect();
        notes.push(format!(
            "load.devices: {} modelled ({}); each meter was subtracted from the house total before the base profile was learned, and is added back above it",
            plan.included.len(),
            names.join(", ")
        ));
    }
    for (device, reason) in &plan.excluded {
        if inputs.holiday && !device.keep_while_away {
            notes.push(format!(
                "load.devices: {} is off in holiday mode",
                device.label()
            ));
            continue;
        }
        let detail = profile
            .and_then(|base| base.devices_excluded.get(&device.key))
            .cloned()
            .unwrap_or_else(|| reason.clone());
        notes.push(format!(
            "load.devices: {} is not modelled because {detail}; it is still inside the base profile, so adding it would count it twice",
            device.label()
        ));
    }
    if let Some(base) = profile {
        if base.devices_dropped_hours > 0 {
            notes.push(format!(
                "load.devices: {} house hours were left out of the base profile because a device meter had no reading for them (a missing reading is unknown, not zero)",
                base.devices_dropped_hours
            ));
        }
        if base.devices_clamped_hours > 0 {
            notes.push(format!(
                "load.devices: on {} house hours the device meters claimed more than the house total, so those hours were left out of the base profile (the meters overlap)",
                base.devices_clamped_hours
            ));
        }
    }
    notes
}
