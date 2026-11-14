//! The operator's load-prediction settings: holiday mode, the weather sources and the named loads.
//!
//! This is a plain payload, not a model: it is stored beside the learned profiles (kind `load`,
//! key `config`) and read by both the learning pass and the serving path, so a setting can never
//! mean one thing at learning time and another at plan time.

use serde_json::{Map, Value};

/// The served bucket quantile when the payload does not say.
pub const DEFAULT_LOAD_QUANTILE: f64 = 0.75;

use super::device::LoadDevice;
use super::parse::parse_devices;

/// `load.devices` plus holiday mode and the two weather sources.
///
/// Every field has a value that keeps the previous behaviour: at home, no weather, no named loads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadConfig {
    /// The house is away: serve the base floor plus only the loads that keep running.
    pub holiday: bool,
    /// Outdoor-temperature sensor entity id (a numeric °C sensor), used for the temperature fits.
    pub temperature_entity: String,
    /// Weather entity whose hourly forecast temperatures serve the horizon.
    pub weather_entity: String,
    /// The house load meter. Empty means the energy dashboard's balance is the house.
    pub load_entity: String,
    /// Which bucket quantile the plan is served, `0.5` (median) to `0.9` (near-peak).
    pub load_quantile: f64,
    /// The configured `load.devices` entries, kept verbatim so an unknown key is not lost.
    pub devices: Vec<Value>,
}

impl LoadConfig {
    /// Read a stored config; every missing key defaults, so an older row keeps working.
    pub fn from_payload(payload: &Value) -> Self {
        let Some(map) = payload.as_object() else {
            return Self::default();
        };
        Self {
            holiday: map.get("holiday").and_then(Value::as_bool).unwrap_or(false),
            temperature_entity: text(map, "temperatureEntity"),
            weather_entity: text(map, "weatherEntity"),
            load_entity: text(map, "loadEntity"),
            load_quantile: number(map, "loadQuantile", DEFAULT_LOAD_QUANTILE),
            devices: map
                .get("devices")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// The payload to store.
    pub fn to_payload(&self) -> Value {
        let mut map = Map::new();
        map.insert("holiday".to_string(), Value::Bool(self.holiday));
        map.insert(
            "temperatureEntity".to_string(),
            Value::String(self.temperature_entity.clone()),
        );
        map.insert(
            "weatherEntity".to_string(),
            Value::String(self.weather_entity.clone()),
        );
        map.insert(
            "loadEntity".to_string(),
            Value::String(self.load_entity.clone()),
        );
        map.insert("loadQuantile".to_string(), Value::from(self.load_quantile));
        map.insert("devices".to_string(), Value::Array(self.devices.clone()));
        Value::Object(map)
    }

    /// The usable named loads, plus one line per entry that was dropped.
    ///
    /// The house load here is the energy dashboard's balance, not one meter, so there is no house
    /// entity to compare against; duplicates and empty entries are still refused.
    pub fn parsed_devices(&self) -> (Vec<LoadDevice>, Vec<String>) {
        parse_devices(&self.devices, "")
    }

    /// The weather sources that were configured, trimmed.
    pub fn temperature_source(&self) -> Option<&str> {
        let entity = self.temperature_entity.trim();
        (!entity.is_empty()).then_some(entity)
    }

    /// The served bucket quantile, clamped to `0.5..=0.9`.
    ///
    /// The median hour is the middle of the house's day, not its peak: serving p50 under-buys
    /// exactly when the oven, the heat pump and the EV line up. The shipped default is the upper
    /// quartile, and the operator can move it back to the median.
    pub fn quantile(&self) -> f64 {
        if !self.load_quantile.is_finite() {
            return DEFAULT_LOAD_QUANTILE;
        }
        self.load_quantile.clamp(0.5, 0.9)
    }

    /// The house load meter, trimmed; `None` means the dashboard balance is the house.
    pub fn load_source(&self) -> Option<&str> {
        let entity = self.load_entity.trim();
        (!entity.is_empty()).then_some(entity)
    }

    /// The weather entity whose forecast serves the horizon, trimmed.
    pub fn weather_source(&self) -> Option<&str> {
        let entity = self.weather_entity.trim();
        (!entity.is_empty()).then_some(entity)
    }
}

fn number(map: &Map<String, Value>, key: &str, fallback: f64) -> f64 {
    map.get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn text(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::LoadConfig;
    use serde_json::json;

    #[test]
    fn a_missing_payload_is_the_old_behaviour() {
        let config = LoadConfig::from_payload(&json!({}));
        assert!(!config.holiday);
        assert!(config.temperature_source().is_none());
        assert!(config.load_source().is_none());
        assert!(config.weather_source().is_none());
        assert!(
            (config.quantile() - 0.75).abs() < 1e-12,
            "the plan serves the upper quartile"
        );
        assert_eq!(config.parsed_devices().0, Vec::new());
    }

    #[test]
    fn a_round_trip_keeps_an_unknown_key_and_the_away_flag() {
        let payload = json!({
            "holiday": true,
            "temperatureEntity": " sensor.outdoor ",
            "weatherEntity": "weather.home",
            "loadEntity": " sensor.home_load_power ",
            "loadQuantile": 0.6,
            "devices": [{"entity": "sensor.heat_pump", "kind": "heat_pump", "keep_while_away": true, "future": 1}],
        });
        let config = LoadConfig::from_payload(&payload);
        assert_eq!(config.temperature_source(), Some("sensor.outdoor"));
        assert_eq!(config.load_source(), Some("sensor.home_load_power"));
        assert!((config.quantile() - 0.6).abs() < 1e-12);
        let (devices, problems) = config.parsed_devices();
        assert!(problems.is_empty(), "{problems:?}");
        assert!(devices[0].keep_while_away);
        assert!(
            devices[0].temperature,
            "a heat pump keeps its temperature term"
        );
        let stored = config.to_payload();
        assert_eq!(stored["devices"][0]["future"], json!(1));
        assert_eq!(LoadConfig::from_payload(&stored), config);
    }
}
