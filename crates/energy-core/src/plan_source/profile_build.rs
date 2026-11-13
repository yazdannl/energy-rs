//! Learning the house's load profile from the energy dashboard's hourly statistics.
//!
//! The profile needs one average kW per UTC hour, and the dashboard's own balance is the only honest
//! source: `import - export + PV + battery discharge - battery charge`. A missing statistic is a gap,
//! not a zero: an hour where any configured counter has no row is skipped, because a zero-filled
//! hour teaches the profile a house that was empty and dark. The battery counters are optional (a
//! dashboard without a battery has none); the grid counters are not.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use energy_forecast::load::{LoadConfig, LoadDevice, LoadProfile, LoadSample, WINDOW_DAYS};
use energy_ha::{Connection, discover, mean_series_hourly, slot_series_hourly};

use crate::plan::PlanError;
use crate::plan_source::LoadProfiles;
use crate::plan_source::device_meter;

/// Rows shorter than this cannot teach a weekday/weekend profile; refuse instead of storing noise.
const MIN_HOURS: usize = 7 * 24;

/// Fetch the dashboard's hourly history, learn the profile and store it.
pub async fn learn(
    connection: &Connection,
    now: i64,
    profiles: &LoadProfiles,
) -> Result<LoadProfile, PlanError> {
    let site = discover(connection)
        .await
        .map_err(|error| PlanError::Supply {
            reason: error.to_string(),
        })?;
    if !site.configured {
        return Err(PlanError::SiteNotConfigured);
    }
    let (from, to) = window(now)?;
    let config = profiles.config();
    let (devices, problems) = config.parsed_devices();
    for problem in &problems {
        tracing::warn!(%problem, "a load.devices entry was ignored");
    }
    let counters = collect(connection, &site, from, to).await?;
    let samples = samples(&counters, from, to);
    if samples.len() < MIN_HOURS {
        return Err(PlanError::Supply {
            reason: format!(
                "only {} of the last {WINDOW_DAYS} days have complete hourly history; a load profile needs {MIN_HOURS} hours",
                samples.len() / 24
            ),
        });
    }
    let temperatures = temperatures(connection, &config, from, to).await;
    let device_samples = device_series(connection, &devices, from, to).await;
    let moment = DateTime::<Utc>::from_timestamp(now, 0).ok_or(PlanError::Supply {
        reason: format!("timestamp {now} is not representable"),
    })?;
    profiles
        .learn(&samples, &temperatures, &devices, &device_samples, moment)
        .map_err(|error| PlanError::Supply {
            reason: format!("load profile could not be stored: {error}"),
        })
}

/// The outdoor temperature by UTC hour, or an empty series when nothing is configured.
///
/// A configured sensor that cannot be read is logged and left empty: the profile is still worth
/// learning without its temperature term, and refusing the whole pass would freeze the buckets
/// too.
async fn temperatures(
    connection: &Connection,
    config: &LoadConfig,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> BTreeMap<DateTime<Utc>, f64> {
    let Some(entity) = config.temperature_source() else {
        return BTreeMap::new();
    };
    match mean_series_hourly(connection, entity, from, to).await {
        Ok(series) => series,
        Err(error) => {
            tracing::warn!(entity, %error, "the outdoor temperature series is unreadable; the temperature term stays off");
            BTreeMap::new()
        }
    }
}

/// One hourly kW series per configured load meter, keyed by device key.
///
/// A meter that cannot be read is absent from the map, which makes the split refuse it with a
/// reason: it stays inside the base profile instead of being subtracted and then lost.
async fn device_series(
    connection: &Connection,
    devices: &[LoadDevice],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> BTreeMap<String, BTreeMap<DateTime<Utc>, f64>> {
    let mut out = BTreeMap::new();
    for device in devices {
        match device_meter::hourly_kwh(connection, &device.entity, from, to).await {
            Ok(series) => {
                out.insert(device.key.clone(), series);
            }
            Err(reason) => {
                tracing::warn!(
                    entity = device.entity,
                    %reason,
                    "a load.devices meter cannot be split off the house; it stays inside the base profile"
                );
            }
        }
    }
    out
}

/// The dashboard's hourly counters: grid (required), PV and battery (optional).
struct Counters {
    import: BTreeMap<i64, f64>,
    export: BTreeMap<i64, f64>,
    pv: BTreeMap<i64, f64>,
    charge: BTreeMap<i64, f64>,
    discharge: BTreeMap<i64, f64>,
}

/// Read every configured hourly statistic over the window.
async fn collect(
    connection: &Connection,
    site: &energy_ha::SiteModel,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Counters, PlanError> {
    let import = series(
        connection,
        site.grid.import.as_ref(),
        "grid_import",
        from,
        to,
    )
    .await?;
    let export = series(
        connection,
        site.grid.export.as_ref(),
        "grid_export",
        from,
        to,
    )
    .await?;
    let pv = optional(connection, site.solar.energy.as_ref(), from, to).await?;
    let (charge_entities, discharge_entities) =
        crate::plan_source::ha_series::battery_entities(site);
    let charge = optional_many(connection, &charge_entities, from, to).await?;
    let discharge = optional_many(connection, &discharge_entities, from, to).await?;
    Ok(Counters {
        import,
        export,
        pv,
        charge,
        discharge,
    })
}

/// The hourly window: `WINDOW_DAYS` before the current hour.
fn window(now: i64) -> Result<(DateTime<Utc>, DateTime<Utc>), PlanError> {
    let moment = DateTime::<Utc>::from_timestamp(now, 0).ok_or(PlanError::Supply {
        reason: format!("timestamp {now} is not representable"),
    })?;
    // Floor to the hour including seconds: the statistics are keyed by hour starts, so a window
    // that kept `:06` would miss every row and learn nothing.
    let end =
        moment - Duration::seconds(i64::from(moment.minute()) * 60 + i64::from(moment.second()));
    Ok((end - Duration::days(i64::from(WINDOW_DAYS)), end))
}

/// One required series as `hour start -> kWh`.
async fn series(
    connection: &Connection,
    entity: Option<&String>,
    name: &'static str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BTreeMap<i64, f64>, PlanError> {
    let entity = entity
        .filter(|id| !id.trim().is_empty())
        .ok_or(PlanError::StatsMissing { series: name })?;
    fetch(connection, entity, from, to).await
}

/// One optional series; an absent entity is an empty series.
async fn optional(
    connection: &Connection,
    entity: Option<&String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BTreeMap<i64, f64>, PlanError> {
    match entity.filter(|id| !id.trim().is_empty()) {
        Some(entity) => fetch(connection, entity, from, to).await,
        None => Ok(BTreeMap::new()),
    }
}

/// Several optional counters, summed per hour; an empty list is an empty series.
async fn optional_many(
    connection: &Connection,
    entities: &[String],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BTreeMap<i64, f64>, PlanError> {
    let mut total = BTreeMap::new();
    for entity in entities {
        for (hour, kwh) in fetch(connection, entity, from, to).await? {
            *total.entry(hour).or_insert(0.0) += kwh;
        }
    }
    Ok(total)
}

/// One statistic as hourly kWh; an hour Home Assistant has no state for is skipped, not zeroed.
async fn fetch(
    connection: &Connection,
    entity: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BTreeMap<i64, f64>, PlanError> {
    let slots = slot_series_hourly(connection, entity, from, to)
        .await
        .map_err(|error| PlanError::Supply {
            reason: error.to_string(),
        })?;
    let mut series = BTreeMap::new();
    for slot in slots {
        let hour = slot.start_ms.div_euclid(1000);
        if let Some(kwh) = slot.kwh {
            series.insert(hour, kwh);
        }
    }
    Ok(series)
}

/// The hourly house load, skipping any hour a configured counter does not cover.
fn samples(counters: &Counters, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<LoadSample> {
    let mut samples = Vec::new();
    let mut hour = from.timestamp();
    while hour < to.timestamp() {
        if let (Some(imported), Some(exported)) =
            (counters.import.get(&hour), counters.export.get(&hour))
        {
            let solar = counters.pv.get(&hour).copied().unwrap_or(0.0);
            let charged = counters.charge.get(&hour).copied().unwrap_or(0.0);
            let discharged = counters.discharge.get(&hour).copied().unwrap_or(0.0);
            if let Some(moment) = DateTime::<Utc>::from_timestamp(hour, 0) {
                samples.push(LoadSample {
                    moment,
                    load_kw: (imported - exported + solar + discharged - charged).max(0.0),
                });
            }
        }
        hour = hour.saturating_add(3600);
    }
    samples
}
