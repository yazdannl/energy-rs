//! Core's `PowerConfig`: exactly one variant is configured.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct PowerConfig {
    #[serde(default)]
    pub stat_rate: Option<String>,
    #[serde(default)]
    pub stat_rate_inverted: Option<String>,
    #[serde(default)]
    pub stat_rate_from: Option<String>,
    #[serde(default)]
    pub stat_rate_to: Option<String>,
}
