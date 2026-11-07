//! The subset of `GET /api/config` this app needs.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct CoreConfig {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub time_zone: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub unit_system: Value,
    #[serde(default)]
    pub state: String,
}
