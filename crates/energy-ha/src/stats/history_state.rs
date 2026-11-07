//! One raw history row and the grouped response.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `minimal_response` uses `s`/`lu`; full responses use `state`/timestamps.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct HistoryState {
    #[serde(default, alias = "s")]
    pub state: Option<String>,
    #[serde(
        default,
        alias = "lu",
        deserialize_with = "crate::state::deserialize_timestamp"
    )]
    pub last_updated: Option<f64>,
    #[serde(
        default,
        alias = "lc",
        deserialize_with = "crate::state::deserialize_timestamp"
    )]
    pub last_changed: Option<f64>,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub attributes: Option<serde_json::Map<String, Value>>,
}

pub type HistoryResponse = BTreeMap<String, Vec<HistoryState>>;
