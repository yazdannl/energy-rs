//! Serving: hourly bands summed, then linearly interpolated to 15-minute slots.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};

use super::bands::{HourlyBands, hourly_bands};
use super::confidence::Confidence;
use super::device::LoadDevice;
use super::inputs::LoadInputs;
use super::plan::{DevicePlan, device_plan};
use super::profile::LoadProfile;
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
    let plan = device_plan(profile, devices, inputs.device_profiles);
    let total = summed_bands(profile, &plan, &grid, inputs);
    LoadForecast {
        slots: slots_from(&grid, &total),
        generated_at: inputs.now,
        source: LoadSource::classify(profile.is_some(), inputs.base_kwh_per_day),
        confidence: Confidence::from_profile(profile),
        notes: notes_for(profile, &plan),
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
    let mut components: Vec<Option<&LoadProfile>> = vec![profile];
    for (_, component) in &plan.included {
        components.push(Some(component));
    }
    for component in components {
        let bands = hourly_bands(
            component,
            first_hour,
            last_hour,
            inputs.zone,
            inputs.base_kwh_per_day,
            inputs.temperatures,
        );
        total.add(&bands);
    }
    total
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

fn notes_for(profile: Option<&LoadProfile>, plan: &DevicePlan) -> Vec<String> {
    let mut notes = Vec::new();
    if profile.is_none() {
        notes.push("cold start: no learned profile yet".to_string());
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
                "load.devices: on {} house hours the device meters summed to more than the house total; the base contribution was floored at zero, which suggests the meters overlap",
                base.devices_clamped_hours
            ));
        }
    }
    notes
}
