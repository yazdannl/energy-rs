//! Request body for `recorder/statistics_during_period`.

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::client::{Connection, HaError};

use super::{Period, StatType, StatisticsResponse};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StatisticsQuery {
    pub start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    pub statistic_ids: Vec<String>,
    pub period: Period,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<StatType>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub units: BTreeMap<String, String>,
}

impl StatisticsQuery {
    pub fn new(
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        statistic_ids: Vec<String>,
        period: Period,
    ) -> Self {
        Self {
            start_time: start.to_rfc3339_opts(SecondsFormat::Secs, true),
            end_time: end.map(|stamp| stamp.to_rfc3339_opts(SecondsFormat::Secs, true)),
            statistic_ids,
            period,
            types: Vec::new(),
            units: BTreeMap::new(),
        }
    }

    pub fn with_types(mut self, types: Vec<StatType>) -> Self {
        self.types = types;
        self
    }

    pub fn with_unit(mut self, unit_class: &str, unit: &str) -> Self {
        self.units.insert(unit_class.to_owned(), unit.to_owned());
        self
    }
}

pub async fn statistics_during_period(
    connection: &Connection,
    query: &StatisticsQuery,
) -> Result<StatisticsResponse, HaError> {
    if query.statistic_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    connection
        .call(
            "recorder/statistics_during_period",
            serde_json::to_value(query)?,
        )
        .await
}
