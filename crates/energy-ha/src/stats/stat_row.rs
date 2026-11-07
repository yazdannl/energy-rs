//! One statistics row and the grouped response.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One statistics row. Timestamps are epoch milliseconds; fields that were not requested (or are
/// impossible for the statistic) are `None`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct StatRow {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub change: Option<f64>,
    #[serde(default)]
    pub mean: Option<f64>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub state: Option<f64>,
    #[serde(default)]
    pub sum: Option<f64>,
    #[serde(default)]
    pub last_reset: Option<i64>,
}

/// Statistics grouped by statistic id.
pub type StatisticsResponse = BTreeMap<String, Vec<StatRow>>;
