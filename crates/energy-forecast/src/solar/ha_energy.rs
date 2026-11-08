//! Home Assistant's `energy/solar_forecast` payload: per-period Wh into an average-kW series.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::series::Series;
use crate::time::parse_iso_utc;

/// The combined Wh points and the number of providers that contributed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HaEnergyPoints {
    /// Wh for the period starting at each stamp, summed across the selected providers.
    pub wh_by_stamp: BTreeMap<DateTime<Utc>, f64>,
    /// Number of providers with a usable `wh_hours` block.
    pub providers: usize,
}

/// Combine the selected providers' `wh_hours` into one `{timestamp: Wh}` map.
///
/// The verified contract is `{config_entry_id: {"wh_hours": {ISO stamp: Wh}}}`. An empty
/// `entry_id` selects every provider and sums their points, so several providers become one series.
pub fn parse_ha_energy_wh(payload: &Value, entry_id: &str) -> HaEnergyPoints {
    let Some(map) = payload.as_object() else {
        return HaEnergyPoints::default();
    };
    let selected: Vec<&Value> = if entry_id.is_empty() {
        map.values().collect()
    } else {
        map.get(entry_id).into_iter().collect()
    };
    let mut out = HaEnergyPoints::default();
    for value in selected {
        let Some(block) = value.get("wh_hours").and_then(Value::as_object) else {
            continue;
        };
        out.providers += 1;
        for (stamp, wh) in block {
            let Some(moment) = parse_iso_utc(stamp) else {
                continue;
            };
            let Some(number) = wh.as_f64() else {
                continue;
            };
            *out.wh_by_stamp.entry(moment).or_insert(0.0) += number;
        }
    }
    out
}

/// Convert per-period Wh into an average-kW series, using each point's own period length.
///
/// The forecast points are Wh for the period starting at the stamp; the next stamp gives that
/// period's length and the last point assumes one hour. A gap far shorter than the series' own
/// resolution is a sunrise/sunset marker rather than a bucket -- Home Assistant appends one with
/// 0 Wh at the exact sunset -- so the bucket before it keeps the nominal length instead of being
/// divided by the marker's seconds (44 Wh over 99 s would read as 1.6 kW at dusk). Periods are
/// clamped to 1 minute .. 3 hours so a sparse or duplicated point cannot fabricate a spike.
pub fn ha_energy_power_series(points: &BTreeMap<DateTime<Utc>, f64>) -> Series {
    let stamps: Vec<DateTime<Utc>> = points.keys().copied().collect();
    let nominal = nominal_hours(&stamps);
    let mut series = Vec::with_capacity(stamps.len());
    for (index, stamp) in stamps.iter().enumerate() {
        let hours = if index + 1 < stamps.len() {
            (stamps[index + 1] - *stamp).num_milliseconds() as f64 / 3_600_000.0
        } else {
            1.0
        };
        let hours = if hours * 4.0 < nominal {
            nominal
        } else {
            hours
        };
        let hours = hours.clamp(1.0 / 60.0, 3.0);
        let wh = points.get(stamp).copied().unwrap_or(0.0);
        series.push((*stamp, (wh / 1000.0 / hours).max(0.0)));
    }
    Series::new(series)
}

/// The series' own resolution: the median gap between points, one hour when it is too short to
/// tell (Home Assistant's forecast buckets are hourly).
fn nominal_hours(stamps: &[DateTime<Utc>]) -> f64 {
    let mut gaps: Vec<f64> = stamps
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_milliseconds() as f64 / 3_600_000.0)
        .filter(|gap| *gap > 0.0)
        .collect();
    if gaps.len() < 4 {
        return 1.0;
    }
    gaps.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let middle = gaps.len() / 2;
    if gaps.len().is_multiple_of(2) {
        gaps[middle - 1].midpoint(gaps[middle])
    } else {
        gaps[middle]
    }
}
