//! Reading the energy dashboard statistics into a validated per-slot history.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use energy_forecast::solar::{Series, ha_energy_power_series};
use energy_forecast::time::parse_iso_utc;
use energy_ha::{Connection, SiteModel, SolarForecast, slot_series_15min};

use crate::plan::{HORIZON_SLOTS, PlanError, SLOT_SECONDS};
use crate::plan_source::MeasuredHistory;

/// The quarter-hour window a supply reads: `hours` before the last complete slot.
pub(crate) fn history_window(now: i64, hours: i64) -> (i64, i64) {
    let end = slot_floor(now);
    let start = end.saturating_sub(hours.max(1).saturating_mul(3600));
    (start, end)
}

/// Floor an instant to its quarter-hour start.
pub(crate) fn slot_floor(now: i64) -> i64 {
    now - now.rem_euclid(SLOT_SECONDS)
}

/// Fetch one statistics series as `start -> kWh`, refusing a hole instead of zero-filling it.
pub(crate) async fn fetch(
    connection: &Connection,
    entity: &str,
    name: &'static str,
    window: (i64, i64),
) -> Result<BTreeMap<i64, f64>, PlanError> {
    let (from, to) = window;
    let slots = slot_series_15min(connection, entity, timestamp(from)?, timestamp(to)?)
        .await
        .map_err(|error| PlanError::Supply {
            reason: error.to_string(),
        })?;
    let mut series = BTreeMap::new();
    for slot in slots {
        let t_start = slot.start_ms.div_euclid(1000);
        let kwh = slot.kwh.ok_or(PlanError::StatsGap {
            series: name,
            t_start,
        })?;
        series.insert(t_start, kwh);
    }
    Ok(series)
}

/// Build the validated PV/load history from the aligned statistics maps.
///
/// Load follows `import - export + PV + discharge - charge`, per slot: the dashboard's own energy
/// balance, never the sum of device meters (which are a subset of the house and would double-count).
pub(crate) fn timeline(
    import: &BTreeMap<i64, f64>,
    export: &BTreeMap<i64, f64>,
    pv: &BTreeMap<i64, f64>,
    charge: &BTreeMap<i64, f64>,
    discharge: &BTreeMap<i64, f64>,
    window: (i64, i64),
) -> Result<MeasuredHistory, PlanError> {
    let (start, end) = window;
    // Statistics for the slot that just ended appear a few seconds late, and a plan is due right at
    // the boundary: trim the unpublished tail instead of refusing a plan for it.
    let end = published_end(import, export, pv, start, end);
    let hours = SLOT_SECONDS as f64 / 3600.0;
    let mut starts = Vec::new();
    let mut pv_kw = Vec::new();
    let mut load_kw = Vec::new();
    let mut t_start = start;
    while t_start < end {
        let imported = get(import, "grid_import", t_start)?;
        let exported = get(export, "grid_export", t_start)?;
        let solar = get(pv, "solar_energy", t_start)?;
        let charged = charge.get(&t_start).copied().unwrap_or(0.0);
        let discharged = discharge.get(&t_start).copied().unwrap_or(0.0);
        starts.push(t_start);
        pv_kw.push(solar / hours);
        load_kw.push((imported - exported + solar + discharged - charged) / hours);
        t_start = t_start.saturating_add(SLOT_SECONDS);
    }
    MeasuredHistory::new(starts, pv_kw, load_kw)
}

/// The window end after dropping the trailing slots Home Assistant has not published yet.
///
/// Only the tail is dropped: an interior hole is still a refusal, because a zero-fill there would
/// teach the planner a house that was empty and dark. A window with nothing published at all comes
/// back empty and is refused by [`MeasuredHistory::new`].
fn published_end(
    import: &BTreeMap<i64, f64>,
    export: &BTreeMap<i64, f64>,
    pv: &BTreeMap<i64, f64>,
    start: i64,
    end: i64,
) -> i64 {
    let mut end = end;
    while end > start {
        let last = end.saturating_sub(SLOT_SECONDS);
        if import.contains_key(&last) && export.contains_key(&last) && pv.contains_key(&last) {
            return end;
        }
        end = last;
    }
    end
}

/// The configured battery charge and discharge statistics, per battery source.
pub(crate) fn battery_entities(site: &SiteModel) -> (Vec<String>, Vec<String>) {
    let mut charge = Vec::new();
    let mut discharge = Vec::new();
    for battery in &site.batteries {
        if let Some(entity) = battery.charge.as_ref().filter(|id| !id.trim().is_empty()) {
            charge.push(entity.clone());
        }
        if let Some(entity) = battery
            .discharge
            .as_ref()
            .filter(|id| !id.trim().is_empty())
        {
            discharge.push(entity.clone());
        }
    }
    (charge, discharge)
}

/// The HA solar forecast as one kW series, or `None` when no provider returned points.
pub(crate) async fn solar_forecast(
    connection: &Connection,
    entry_ids: &[String],
) -> Result<Option<Series>, PlanError> {
    let payload = energy_ha::fetch_solar_forecast(connection)
        .await
        .map_err(|error| PlanError::Supply {
            reason: error.to_string(),
        })?;
    let points = forecast_points(&payload, entry_ids);
    if points.is_empty() {
        return Ok(None);
    }
    Ok(Some(ha_energy_power_series(&points)))
}

/// Sample a series at the quarter-hour starts the horizon needs.
pub(crate) fn sample(series: &Series, from: i64, slots: usize) -> Vec<f64> {
    (0..slots)
        .map(|index| {
            let offset = i64::try_from(index).unwrap_or(0) * SLOT_SECONDS;
            let moment = DateTime::<Utc>::from_timestamp(from.saturating_add(offset), 0);
            moment.map_or(0.0, |moment| series.interpolate(moment).max(0.0))
        })
        .collect()
}

/// The horizon end instant for the price lookup (one slot past the last one).
pub(crate) fn price_end(from: i64) -> i64 {
    let slots = i64::try_from(HORIZON_SLOTS)
        .unwrap_or(i64::MAX)
        .saturating_add(1);
    from.saturating_add(slots.saturating_mul(SLOT_SECONDS))
}

/// Sum the selected providers' `wh_hours` points; an empty selector means every provider.
fn forecast_points(payload: &SolarForecast, entry_ids: &[String]) -> BTreeMap<DateTime<Utc>, f64> {
    let mut points = BTreeMap::new();
    for (id, entry) in payload {
        let selected = entry_ids.is_empty() || entry_ids.iter().any(|wanted| wanted == id);
        if !selected || entry.wh_hours.is_empty() {
            continue;
        }
        for (stamp, wh) in &entry.wh_hours {
            if let Some(moment) = parse_iso_utc(stamp) {
                *points.entry(moment).or_insert(0.0) += wh;
            }
        }
    }
    points
}

fn get(series: &BTreeMap<i64, f64>, name: &'static str, t_start: i64) -> Result<f64, PlanError> {
    series.get(&t_start).copied().ok_or(PlanError::StatsGap {
        series: name,
        t_start,
    })
}

fn timestamp(seconds: i64) -> Result<DateTime<Utc>, PlanError> {
    DateTime::<Utc>::from_timestamp(seconds, 0).ok_or(PlanError::Supply {
        reason: format!("timestamp {seconds} is not representable"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use super::{SLOT_SECONDS, published_end, timeline};

    const START: i64 = 1_790_000_000 - 1_790_000_000 % SLOT_SECONDS;

    /// A series covering `slots` quarter-hours from `START`.
    fn series(slots: i64) -> BTreeMap<i64, f64> {
        (0..slots)
            .map(|index| (START + index * SLOT_SECONDS, 1.0))
            .collect()
    }

    #[test]
    fn the_unpublished_tail_is_trimmed_not_refused() {
        let window = (START, START + 4 * SLOT_SECONDS);
        let end = published_end(&series(3), &series(3), &series(3), window.0, window.1);
        assert_eq!(
            end,
            START + 3 * SLOT_SECONDS,
            "the last published slot wins"
        );
        let history = timeline(
            &series(3),
            &series(3),
            &series(3),
            &BTreeMap::new(),
            &BTreeMap::new(),
            window,
        )
        .expect("a lagging publication is not a refusal");
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn an_interior_hole_is_still_refused() {
        let window = (START, START + 3 * SLOT_SECONDS);
        let mut holed = series(3);
        holed.remove(&(START + SLOT_SECONDS));
        let error = timeline(
            &holed,
            &series(3),
            &series(3),
            &BTreeMap::new(),
            &BTreeMap::new(),
            window,
        )
        .expect_err("an interior gap must refuse");
        assert!(error.to_string().contains("gap"), "{error}");
    }
}
