//! The dashboard preferences document and the discovery entry points.

use serde::Deserialize;
use serde_json::Value;

use crate::client::{Connection, HaError};

use super::{DeviceConsumption, EnergySource, SiteModel};

/// The dashboard preferences document (the `result` of `energy/get_prefs`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct EnergyPreferences {
    #[serde(default)]
    pub energy_sources: Vec<EnergySource>,
    #[serde(default)]
    pub device_consumption: Vec<DeviceConsumption>,
    #[serde(default)]
    pub device_consumption_water: Vec<DeviceConsumption>,
}

/// `energy/get_prefs`; `Ok(None)` when the dashboard is not configured.
pub async fn fetch_prefs(connection: &Connection) -> Result<Option<EnergyPreferences>, HaError> {
    match connection
        .call::<EnergyPreferences>("energy/get_prefs", Value::Null)
        .await
    {
        Ok(prefs) => Ok(Some(prefs)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error),
    }
}

/// Fetch and normalize the dashboard mapping.
pub async fn discover(connection: &Connection) -> Result<SiteModel, HaError> {
    Ok(fetch_prefs(connection)
        .await?
        .map(|prefs| SiteModel::from_prefs(&prefs))
        .unwrap_or_else(SiteModel::not_configured))
}
