//! Reading the energy dashboard's statistics into savings slots and the annual meter totals.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use energy_ha::{Connection, SiteModel, slot_series_15min, slot_series_hourly};
use energy_price::book::query::local_midnight;

use crate::plan::SLOT_SECONDS;

use super::annual_seeds::daily_totals;
use super::{SavingsError, SlotFlow};

/// One measured local day: five dashboard statistics on the quarter-hour grid.
///
/// A missing row stays `None` (a gap, never a zero); a dashboard without a battery counter reports
/// `Some(0.0)` for both battery flows, while a configured counter with a hole is a gap.
pub(crate) async fn day_slots(
    connection: &Connection,
    site: &SiteModel,
    from: i64,
    to: i64,
) -> Result<Vec<SlotFlow>, SavingsError> {
    let import = series(
        connection,
        site.grid.import.as_deref(),
        "grid_import",
        from,
        to,
    )
    .await?;
    let export = series(
        connection,
        site.grid.export.as_deref(),
        "grid_export",
        from,
        to,
    )
    .await?;
    let pv = series(
        connection,
        site.solar.energy.as_deref(),
        "solar_energy",
        from,
        to,
    )
    .await?;
    let (charge_ids, discharge_ids) = battery_entities(site);
    let measured_battery = !charge_ids.is_empty() || !discharge_ids.is_empty();
    let charge = battery_series(connection, &charge_ids, "battery_charge", from, to).await?;
    let discharge =
        battery_series(connection, &discharge_ids, "battery_discharge", from, to).await?;
    let mut slots = Vec::new();
    let mut t_start = from;
    while t_start < to {
        slots.push(SlotFlow {
            t_start,
            import_kwh: import.get(&t_start).copied(),
            export_kwh: export.get(&t_start).copied(),
            pv_kwh: pv.get(&t_start).copied(),
            charge_kwh: battery_value(&charge, t_start, measured_battery),
            discharge_kwh: battery_value(&discharge, t_start, measured_battery),
        });
        t_start = t_start.saturating_add(SLOT_SECONDS);
    }
    Ok(slots)
}

/// Hourly import/export totals per local day for `from ..= through`.
pub(crate) async fn year_totals(
    connection: &Connection,
    site: &SiteModel,
    from: NaiveDate,
    through: NaiveDate,
) -> Result<BTreeMap<NaiveDate, (f64, f64)>, SavingsError> {
    let start = local_midnight(from).ok_or(SavingsError::Clock)?;
    let end = through
        .succ_opt()
        .and_then(local_midnight)
        .ok_or(SavingsError::Clock)?;
    let import = hourly(
        connection,
        site.grid.import.as_deref(),
        "grid_import",
        start,
        end,
    )
    .await?;
    let export = hourly(
        connection,
        site.grid.export.as_deref(),
        "grid_export",
        start,
        end,
    )
    .await?;
    Ok(daily_totals(&import, &export))
}

/// The configured battery energy counters (charge and discharge), per device.
fn battery_entities(site: &SiteModel) -> (Vec<String>, Vec<String>) {
    let mut charge = Vec::new();
    let mut discharge = Vec::new();
    for battery in &site.batteries {
        if let Some(id) = battery.charge.as_ref().filter(|id| !id.trim().is_empty()) {
            charge.push(id.clone());
        }
        if let Some(id) = battery
            .discharge
            .as_ref()
            .filter(|id| !id.trim().is_empty())
        {
            discharge.push(id.clone());
        }
    }
    (charge, discharge)
}

/// One battery counter across the fleet: `Some(0.0)` when the dashboard has no battery counter, a
/// gap when a configured counter is missing.
fn battery_value(series: &BTreeMap<i64, f64>, t_start: i64, measured: bool) -> Option<f64> {
    if measured {
        series.get(&t_start).copied()
    } else {
        Some(0.0)
    }
}

/// The fleet's summed charge or discharge per slot; a hole in any device's counter is a hole.
async fn battery_series(
    connection: &Connection,
    entities: &[String],
    role: &'static str,
    from: i64,
    to: i64,
) -> Result<BTreeMap<i64, f64>, SavingsError> {
    let mut parts = Vec::with_capacity(entities.len());
    for entity in entities {
        parts.push(quarter_hours(connection, entity, role, from, to).await?);
    }
    let Some(first) = parts.first() else {
        return Ok(BTreeMap::new());
    };
    let mut total = BTreeMap::new();
    for (&t_start, &kwh) in first {
        let mut sum = kwh;
        let mut complete = true;
        for other in &parts[1..] {
            if let Some(value) = other.get(&t_start) {
                sum += value;
            } else {
                complete = false;
                break;
            }
        }
        if complete {
            total.insert(t_start, sum);
        }
    }
    Ok(total)
}

/// A required statistic as `start -> kWh`; a missing row is simply absent from the map.
async fn series(
    connection: &Connection,
    entity: Option<&str>,
    role: &'static str,
    from: i64,
    to: i64,
) -> Result<BTreeMap<i64, f64>, SavingsError> {
    let Some(entity) = entity.filter(|id| !id.trim().is_empty()) else {
        return Err(SavingsError::Measurement {
            series: role,
            reason: "not configured in the energy dashboard".to_owned(),
        });
    };
    quarter_hours(connection, entity, role, from, to).await
}

async fn quarter_hours(
    connection: &Connection,
    entity: &str,
    role: &'static str,
    from: i64,
    to: i64,
) -> Result<BTreeMap<i64, f64>, SavingsError> {
    let (start, end) = (instant(from)?, instant(to)?);
    let slots = slot_series_15min(connection, entity, start, end)
        .await
        .map_err(|error| SavingsError::Measurement {
            series: role,
            reason: error.to_string(),
        })?;
    Ok(slots
        .into_iter()
        .filter_map(|slot| Some((slot.start_ms.div_euclid(1_000), slot.kwh?)))
        .collect())
}

async fn hourly(
    connection: &Connection,
    entity: Option<&str>,
    role: &'static str,
    from: i64,
    to: i64,
) -> Result<BTreeMap<i64, f64>, SavingsError> {
    let Some(entity) = entity.filter(|id| !id.trim().is_empty()) else {
        return Ok(BTreeMap::new());
    };
    let (start, end) = (instant(from)?, instant(to)?);
    let slots = slot_series_hourly(connection, entity, start, end)
        .await
        .map_err(|error| SavingsError::Measurement {
            series: role,
            reason: error.to_string(),
        })?;
    Ok(slots
        .into_iter()
        .filter_map(|slot| Some((slot.start_ms.div_euclid(1_000), slot.kwh?)))
        .collect())
}

fn instant(secs: i64) -> Result<DateTime<Utc>, SavingsError> {
    DateTime::from_timestamp(secs, 0).ok_or(SavingsError::Clock)
}
