//! Solar forecast per config-entry id and its typed call.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::client::{Connection, HaError};

/// `Wh` for the period starting at each timestamp, keyed by opaque config-entry id.
pub type SolarForecast = BTreeMap<String, SolarForecastEntry>;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SolarForecastEntry {
    #[serde(default)]
    pub wh_hours: BTreeMap<String, f64>,
}

/// `energy/solar_forecast` (may be empty or partial; missing providers are not an error).
pub async fn fetch_solar_forecast(connection: &Connection) -> Result<SolarForecast, HaError> {
    connection.call("energy/solar_forecast", Value::Null).await
}
