//! Typed `config/device_registry/list` access.

use serde::Deserialize;
use serde_json::Value;

use crate::client::{Connection, HaError};

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct DeviceRegistryEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub name_by_user: Option<String>,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub sw_version: Option<String>,
    #[serde(default)]
    pub hw_version: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
    #[serde(default)]
    pub area_id: Option<String>,
    #[serde(default)]
    pub disabled_by: Option<String>,
    #[serde(default)]
    pub entry_type: Option<String>,
    #[serde(default)]
    pub config_entry_id: Option<String>,
    #[serde(default)]
    pub config_entries: Vec<String>,
    #[serde(default)]
    pub via_device_id: Option<String>,
    #[serde(default)]
    pub parent_device_id: Option<String>,
    #[serde(default)]
    pub identifiers: Vec<Vec<String>>,
    #[serde(default)]
    pub connections: Vec<Vec<String>>,
    #[serde(default)]
    pub created_at: Option<f64>,
    #[serde(default)]
    pub modified_at: Option<f64>,
    #[serde(default)]
    pub labels: Vec<String>,
}

pub async fn device_registry(connection: &Connection) -> Result<Vec<DeviceRegistryEntry>, HaError> {
    connection
        .call("config/device_registry/list", Value::Null)
        .await
}
