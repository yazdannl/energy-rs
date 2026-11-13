//! Hourly *mean* series from recorder statistics: temperatures, not energy counters.

use std::collections::BTreeMap;

use chrono::{DateTime, Timelike, Utc};
use serde_json::Value;

use crate::client::{Connection, HaError};
use crate::stats::{Period, StatType, StatisticsQuery, statistics_during_period};

/// An hourly mean series, keyed by the UTC hour it covers.
///
/// A bucket the recorder has no `mean` for is missing, never zero: a zero would read as a freezing
/// night and teach the temperature fit a slope that does not exist.
pub async fn mean_series_hourly(
    connection: &Connection,
    entity_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BTreeMap<DateTime<Utc>, f64>, HaError> {
    if to <= from {
        return Ok(BTreeMap::new());
    }
    let query = StatisticsQuery::new(from, Some(to), vec![entity_id.to_owned()], Period::Hour)
        .with_types(vec![StatType::Mean]);
    let response = statistics_during_period(connection, &query).await?;
    let mut series = BTreeMap::new();
    for row in response.get(entity_id).map_or(&[][..], Vec::as_slice) {
        let Some(mean) = row.mean.filter(|value| value.is_finite()) else {
            continue;
        };
        if let Some(hour) = DateTime::<Utc>::from_timestamp_millis(row.start) {
            series.insert(hour, mean);
        }
    }
    Ok(series)
}

/// `weather.get_forecasts` hourly temperatures, keyed by the UTC hour of each entry.
///
/// A weather entity without a response (or without an hourly forecast) is an empty series: the
/// caller holds the last known temperature instead of refusing the plan.
pub async fn weather_forecast_hourly(
    connection: &Connection,
    entity_id: &str,
) -> Result<BTreeMap<DateTime<Utc>, f64>, HaError> {
    let params = serde_json::json!({
        "domain": "weather",
        "service": "get_forecasts",
        "service_data": {"type": "hourly", "entity_id": entity_id},
        "return_response": true,
    });
    let value = match connection.call_value("call_service", params).await {
        Ok(value) => value,
        // An entity that has no such service answers with an API error; that is a missing
        // forecast, not a broken plan.
        Err(error) if error.is_not_found() || error.is_unknown_command() => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(error),
    };
    Ok(parse_forecast(&value))
}

fn parse_forecast(value: &Value) -> BTreeMap<DateTime<Utc>, f64> {
    let mut series = BTreeMap::new();
    let Some(entries) = value.get("response").and_then(Value::as_object) else {
        return series;
    };
    for entry in entries.values() {
        let Some(forecast) = entry.get("forecast").and_then(Value::as_array) else {
            continue;
        };
        for item in forecast {
            let Some(stamp) = item.get("datetime").and_then(Value::as_str) else {
                continue;
            };
            let temperature = item
                .get("temperature")
                .or_else(|| item.get("native_temperature"))
                .and_then(Value::as_f64);
            let (Some(temperature), Ok(moment)) =
                (temperature, DateTime::parse_from_rfc3339(stamp))
            else {
                continue;
            };
            let hour = moment
                .with_timezone(&Utc)
                .with_minute(0)
                .and_then(|value| value.with_second(0));
            if let Some(hour) = hour.and_then(|value| value.with_nanosecond(0)) {
                series.insert(hour, temperature);
            }
        }
    }
    series
}

/// Home Assistant's own coordinates (`get_config`), which the weather calls need.
///
/// `(0, 0)` is the null island: Home Assistant reports it when the user never set a location, and a
/// forecast for it is worse than no forecast at all.
pub async fn core_location(connection: &Connection) -> Result<Option<(f64, f64)>, HaError> {
    let value = connection
        .call_value("get_config", serde_json::json!({}))
        .await?;
    Ok(parse_location(&value))
}

fn parse_location(value: &Value) -> Option<(f64, f64)> {
    let latitude = value.get("latitude")?.as_f64()?;
    let longitude = value.get("longitude")?.as_f64()?;
    if !latitude.is_finite() || !longitude.is_finite() {
        return None;
    }
    if latitude == 0.0 && longitude == 0.0 {
        return None;
    }
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    Some((latitude, longitude))
}

/// How long one Open-Meteo answer is reused before another request is made.
const OPEN_METEO_TTL: std::time::Duration = std::time::Duration::from_mins(30);

/// The last Open-Meteo answer and when it was fetched.
static OPEN_METEO: std::sync::OnceLock<
    std::sync::Mutex<Option<(std::time::Instant, OpenMeteoSeries)>>,
> = std::sync::OnceLock::new();

/// An Open-Meteo hourly temperature series and the instant it was fetched.
type OpenMeteoSeries = BTreeMap<DateTime<Utc>, f64>;

/// Open-Meteo's hourly temperature for the next three days, by UTC hour.
///
/// No API key, no account: the free endpoint takes a coordinate and returns JSON. Answers are
/// cached for [`OPEN_METEO_TTL`] because the plan is rebuilt every few minutes and the provider
/// asks for reasonable use.
pub async fn open_meteo_hourly(latitude: f64, longitude: f64) -> Result<OpenMeteoSeries, HaError> {
    let cache = OPEN_METEO.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some((fetched, series)) = guard.as_ref()
        && fetched.elapsed() < OPEN_METEO_TTL
    {
        return Ok(series.clone());
    }
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={latitude:.4}&longitude={longitude:.4}\
         &hourly=temperature_2m&forecast_days=3&timezone=UTC&temperature_unit=celsius"
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("energy-rs/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| HaError::Http(error.to_string()))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| HaError::Http(error.to_string()))?;
    if !response.status().is_success() {
        return Err(HaError::Http(format!(
            "Open-Meteo answered {}",
            response.status()
        )));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| HaError::Http(error.to_string()))?;
    let series = parse_open_meteo(&body);
    if series.is_empty() {
        return Err(HaError::Http(
            "Open-Meteo returned no hourly temperatures".to_owned(),
        ));
    }
    tracing::info!(
        hours = series.len(),
        latitude,
        longitude,
        "Open-Meteo hourly temperatures fetched"
    );
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((std::time::Instant::now(), series.clone()));
    }
    Ok(series)
}

fn parse_open_meteo(value: &Value) -> OpenMeteoSeries {
    let Some(hourly) = value.get("hourly") else {
        return BTreeMap::new();
    };
    let (Some(times), Some(temperatures)) = (
        hourly.get("time").and_then(Value::as_array),
        hourly.get("temperature_2m").and_then(Value::as_array),
    ) else {
        return BTreeMap::new();
    };
    let mut series = BTreeMap::new();
    for (stamp, temperature) in times.iter().zip(temperatures.iter()) {
        let (Some(stamp), Some(temperature)) = (stamp.as_str(), temperature.as_f64()) else {
            continue;
        };
        if !temperature.is_finite() {
            continue;
        }
        // `timezone=UTC` makes the stamps naive UTC (`2026-11-13T00:00`).
        let Ok(moment) = chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%M") else {
            continue;
        };
        series.insert(moment.and_utc(), temperature);
    }
    series
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{parse_location, parse_open_meteo};
    use serde_json::json;

    #[test]
    fn an_open_meteo_body_becomes_utc_hours() {
        let body = json!({
            "hourly": {
                "time": ["2026-11-13T00:00", "2026-11-13T01:00"],
                "temperature_2m": [7.1, null]
            }
        });
        let series = parse_open_meteo(&body);
        assert_eq!(series.len(), 1, "a null temperature is a gap, not zero");
        assert!((series.values().next().copied().unwrap() - 7.1).abs() < 1e-9);
    }

    #[test]
    fn a_missing_or_null_island_location_is_refused() {
        assert_eq!(
            parse_location(&json!({"latitude": 52.1, "longitude": 4.3})),
            Some((52.1, 4.3))
        );
        assert_eq!(
            parse_location(&json!({"latitude": 0, "longitude": 0})),
            None
        );
        assert_eq!(
            parse_location(&json!({"latitude": 200, "longitude": 4.3})),
            None
        );
        assert_eq!(parse_location(&json!({})), None);
    }
}
