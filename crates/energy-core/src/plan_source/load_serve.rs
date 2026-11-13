//! The load forecast the plan is served: the learned profile, plus the named loads and the
//! outdoor temperature that ride on top of it.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use energy_forecast::load::{
    LoadConfig, LoadDevice, LoadForecast, LoadInputs, LoadProfile, load_forecast,
};
use energy_ha::Connection;
use energy_price::book::query::TARIFF_TZ;

use crate::plan::PlanError;
use crate::plan_source::ha_series;

/// The learned profiles and weather a served forecast is built from.
pub(crate) struct Served<'a> {
    pub devices: &'a [LoadDevice],
    pub device_profiles: &'a BTreeMap<String, LoadProfile>,
    pub temperatures: &'a BTreeMap<DateTime<Utc>, f64>,
    pub holiday: bool,
}

/// The load series at `now`, from the learned profile or the residential fallback shape.
///
/// The forecast's own notes (which loads are modelled, what the split had to drop, whether the
/// temperature term is on) are appended to `notes`, which the plan publishes as its `degraded`
/// string: a forecast that is not what the operator configured must say so.
pub(crate) fn serve(
    profile: Option<&LoadProfile>,
    base_kwh_per_day: f64,
    now: i64,
    slots: usize,
    served: &Served<'_>,
    notes: &mut Vec<String>,
) -> Result<Vec<f64>, PlanError> {
    let moment = DateTime::<Utc>::from_timestamp(now, 0).ok_or(PlanError::Supply {
        reason: format!("timestamp {now} is not representable"),
    })?;
    let inputs = LoadInputs::new(
        served.device_profiles,
        served.temperatures,
        base_kwh_per_day,
        moment,
        TARIFF_TZ,
        Some(30),
    )
    .with_holiday(served.holiday);
    let forecast: LoadForecast = load_forecast(profile, served.devices, &inputs);
    notes.extend(forecast.notes.iter().cloned());
    if forecast.slots.len() < slots {
        return Err(PlanError::ForecastUnavailable {
            name: "load_kw",
            reason: format!(
                "load forecast served {} slots, the horizon needs {slots}",
                forecast.slots.len()
            ),
        });
    }
    Ok(forecast
        .slots
        .iter()
        .take(slots)
        .map(|slot| slot.load_kw.max(0.0))
        .collect())
}

/// The weather one plan uses: the PV model's hourly fields, the temperatures the load model reads,
/// and the coordinates both came from.
pub(crate) struct Weather {
    /// Open-Meteo's hourly weather (empty when it answered nothing).
    pub hours: BTreeMap<DateTime<Utc>, energy_ha::OpenMeteoHour>,
    /// The temperature by UTC hour the load regression reads.
    pub temperatures: BTreeMap<DateTime<Utc>, f64>,
    /// Home Assistant's coordinates, when it has a location.
    pub location: Option<(f64, f64)>,
}

/// Fetch the weather once: the configured entity first, then Open-Meteo, then the measured hour.
pub(crate) async fn weather(
    connection: &Connection,
    config: &LoadConfig,
    now: i64,
    slots: usize,
) -> Weather {
    let location = energy_ha::core_location(connection).await.ok().flatten();
    let hours = meteo_hours(location).await;
    let from_meteo: BTreeMap<DateTime<Utc>, f64> = hours
        .iter()
        .map(|(hour, weather)| (*hour, weather.temperature_c))
        .collect();
    let temperatures = match entity_temperatures(connection, config).await {
        Some(series) => series,
        None if !from_meteo.is_empty() => from_meteo,
        None => match config.temperature_source() {
            Some(entity) => held_temperatures(connection, entity, now, slots).await,
            None => BTreeMap::new(),
        },
    };
    Weather {
        hours,
        temperatures,
        location,
    }
}

/// Open-Meteo's hourly weather over Home Assistant's coordinates (empty when either is missing).
async fn meteo_hours(
    location: Option<(f64, f64)>,
) -> BTreeMap<DateTime<Utc>, energy_ha::OpenMeteoHour> {
    let Some((latitude, longitude)) = location else {
        return BTreeMap::new();
    };
    match energy_ha::open_meteo_forecast(latitude, longitude).await {
        Ok(series) => series,
        Err(error) => {
            tracing::warn!(%error, "the Open-Meteo weather forecast is unreadable");
            BTreeMap::new()
        }
    }
}

/// The configured weather entity's hourly forecast, or `None` when it has none.
async fn entity_temperatures(
    connection: &Connection,
    config: &LoadConfig,
) -> Option<BTreeMap<DateTime<Utc>, f64>> {
    let entity = config.weather_source()?;
    match energy_ha::weather_forecast_hourly(connection, entity).await {
        Ok(series) if !series.is_empty() => Some(series),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(entity, %error, "the weather forecast is unreadable");
            None
        }
    }
}

/// The newest measured hour, held flat across the horizon.
///
/// A measured sensor says what today is, not what tomorrow will be, but a flat line still carries
/// the heat pump's response to the current weather.
async fn held_temperatures(
    connection: &Connection,
    entity: &str,
    now: i64,
    slots: usize,
) -> BTreeMap<DateTime<Utc>, f64> {
    let start = ha_series::slot_floor(now);
    let (Some(from), Some(to)) = (
        DateTime::<Utc>::from_timestamp(start - 7 * 24 * 3600, 0),
        DateTime::<Utc>::from_timestamp(start, 0),
    ) else {
        return BTreeMap::new();
    };
    let series = match energy_ha::mean_series_hourly(connection, entity, from, to).await {
        Ok(series) => series,
        Err(error) => {
            tracing::warn!(entity, %error, "the outdoor temperature series is unreadable");
            return BTreeMap::new();
        }
    };
    let Some(value) = series.values().next_back().copied() else {
        return BTreeMap::new();
    };
    let mut temperatures = BTreeMap::new();
    let hours = i64::try_from(slots.div_ceil(4) + 2).unwrap_or(i64::MAX);
    let mut cursor = start;
    for _ in 0..hours {
        if let Some(moment) = DateTime::<Utc>::from_timestamp(cursor, 0) {
            temperatures.insert(moment, value);
        }
        cursor = cursor.saturating_add(3600);
    }
    temperatures
}
