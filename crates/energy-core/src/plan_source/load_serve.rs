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

/// The outdoor temperature over the horizon: the weather forecast when one is configured,
/// otherwise the newest measured hour held flat.
pub(crate) async fn temperatures(
    connection: &Connection,
    config: &LoadConfig,
    now: i64,
    slots: usize,
) -> BTreeMap<DateTime<Utc>, f64> {
    if let Some(entity) = config.weather_source() {
        match energy_ha::weather_forecast_hourly(connection, entity).await {
            Ok(series) if !series.is_empty() => return series,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(entity, %error, "the weather forecast is unreadable");
            }
        }
    }
    match config.temperature_source() {
        Some(entity) => held_temperatures(connection, entity, now, slots).await,
        None => BTreeMap::new(),
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
