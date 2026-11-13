//! Hourly *mean* series from recorder statistics: temperatures, not energy counters.

use std::collections::BTreeMap;

use chrono::{DateTime, Timelike, Utc};
use serde_json::Value;

use crate::client::{Connection, HaError};
use crate::stats::{Period, StatType, StatisticsQuery, statistics_during_period};

mod open_meteo;

pub use open_meteo::{OpenMeteoHour, open_meteo_forecast, open_meteo_hourly};

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::parse_location;
    use serde_json::json;

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
