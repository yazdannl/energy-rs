//! The Open-Meteo request: hourly weather for the horizon, free and keyless.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::client::HaError;

/// How long one Open-Meteo answer is reused before another request is made.
const OPEN_METEO_TTL: std::time::Duration = std::time::Duration::from_mins(30);

/// One hour of the Open-Meteo forecast: everything the PV model and the load model read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenMeteoHour {
    /// Ambient air temperature, degrees C.
    pub temperature_c: f64,
    /// Cloud coverage, percent.
    pub cloud_pct: f64,
    /// Wind speed at 10 m, m/s.
    pub wind_ms: f64,
    /// Relative humidity, percent.
    pub rh_pct: f64,
    /// Precipitation, mm.
    pub rain_mm: f64,
    /// Global horizontal irradiance, W/m² (the attenuated value, nothing further applied).
    pub ghi_wm2: f64,
}

/// One cached answer: when it was fetched and the hours it carried.
type Cached = (std::time::Instant, BTreeMap<DateTime<Utc>, OpenMeteoHour>);

/// The last Open-Meteo answer and when it was fetched.
static OPEN_METEO: std::sync::OnceLock<std::sync::Mutex<Option<Cached>>> =
    std::sync::OnceLock::new();

/// Open-Meteo's hourly weather for the next three days, by UTC hour.
///
/// No API key, no account: the free endpoint takes a coordinate and returns JSON. Answers are
/// cached for [`OPEN_METEO_TTL`] because the plan is rebuilt every few minutes and the provider
/// asks for reasonable use. Wind is requested in m/s (the endpoint's default is km/h).
pub async fn open_meteo_forecast(
    latitude: f64,
    longitude: f64,
) -> Result<BTreeMap<DateTime<Utc>, OpenMeteoHour>, HaError> {
    let cache = OPEN_METEO.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some((fetched, series)) = guard.as_ref()
        && fetched.elapsed() < OPEN_METEO_TTL
    {
        return Ok(series.clone());
    }
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={latitude:.4}&longitude={longitude:.4}         &hourly=temperature_2m,cloud_cover,wind_speed_10m,relative_humidity_2m,precipitation,         shortwave_radiation&forecast_days=3&timezone=UTC&temperature_unit=celsius         &wind_speed_unit=ms"
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
            "Open-Meteo returned no hourly weather".to_owned(),
        ));
    }
    tracing::info!(
        hours = series.len(),
        latitude,
        longitude,
        "Open-Meteo hourly weather fetched"
    );
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((std::time::Instant::now(), series.clone()));
    }
    Ok(series)
}

/// Open-Meteo's hourly temperatures alone, by UTC hour.
pub async fn open_meteo_hourly(
    latitude: f64,
    longitude: f64,
) -> Result<BTreeMap<DateTime<Utc>, f64>, HaError> {
    Ok(open_meteo_forecast(latitude, longitude)
        .await?
        .into_iter()
        .map(|(hour, weather)| (hour, weather.temperature_c))
        .collect())
}

fn parse_open_meteo(value: &Value) -> BTreeMap<DateTime<Utc>, OpenMeteoHour> {
    let Some(hourly) = value.get("hourly").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    let times = hourly.get("time").and_then(Value::as_array);
    let Some(times) = times else {
        return BTreeMap::new();
    };
    let column = |name: &str| -> Vec<Option<f64>> {
        hourly
            .get(name)
            .and_then(Value::as_array)
            .map(|values| values.iter().map(Value::as_f64).collect())
            .unwrap_or_default()
    };
    let temperature = column("temperature_2m");
    let cloud = column("cloud_cover");
    let wind = column("wind_speed_10m");
    let humidity = column("relative_humidity_2m");
    let rain = column("precipitation");
    let ghi = column("shortwave_radiation");
    let mut series = BTreeMap::new();
    for (index, stamp) in times.iter().enumerate() {
        let Some(stamp) = stamp.as_str() else {
            continue;
        };
        let Ok(moment) = chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%M") else {
            continue;
        };
        let temperature_c = temperature.get(index).copied().flatten();
        let Some(temperature_c) = temperature_c else {
            continue;
        };
        let at = |values: &Vec<Option<f64>>| values.get(index).copied().flatten().unwrap_or(0.0);
        series.insert(
            moment.and_utc(),
            OpenMeteoHour {
                temperature_c,
                cloud_pct: at(&cloud).clamp(0.0, 100.0),
                wind_ms: at(&wind).max(0.0),
                rh_pct: at(&humidity).clamp(0.0, 100.0),
                rain_mm: at(&rain).max(0.0),
                ghi_wm2: at(&ghi).max(0.0),
            },
        );
    }
    series
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::parse_open_meteo;
    use serde_json::json;

    #[test]
    fn an_open_meteo_body_becomes_utc_hours() {
        let body = json!({
            "hourly": {
                "time": ["2026-11-13T00:00", "2026-11-13T01:00"],
                "temperature_2m": [7.1, null],
                "cloud_cover": [42, 55],
                "wind_speed_10m": [3.5, 4.0],
                "relative_humidity_2m": [81, 82],
                "precipitation": [0.0, 0.2],
                "shortwave_radiation": [0.0, 12.5]
            }
        });
        let series = parse_open_meteo(&body);
        assert_eq!(series.len(), 1, "a null temperature is a gap, not zero");
        let hour = series.values().next().copied().unwrap();
        assert!((hour.temperature_c - 7.1).abs() < 1e-9);
        assert!((hour.cloud_pct - 42.0).abs() < 1e-9);
        assert!((hour.wind_ms - 3.5).abs() < 1e-9);
        assert!((hour.ghi_wm2 - 0.0).abs() < 1e-9);
    }
}
