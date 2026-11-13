//! Statistic classification and enumeration.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::client::{Connection, HaError};

use super::StatisticKind;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct StatisticMetadata {
    pub statistic_id: String,
    #[serde(default)]
    pub display_unit_of_measurement: Option<String>,
    #[serde(default)]
    pub has_mean: bool,
    #[serde(default)]
    pub mean_type: u8,
    #[serde(default)]
    pub has_sum: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub statistics_unit_of_measurement: Option<String>,
    #[serde(default)]
    pub unit_class: Option<String>,
}

pub async fn statistics_metadata(
    connection: &Connection,
    statistic_ids: &[String],
) -> Result<Vec<StatisticMetadata>, HaError> {
    if statistic_ids.is_empty() {
        return Ok(Vec::new());
    }
    connection
        .call(
            "recorder/get_statistics_metadata",
            json!({ "statistic_ids": statistic_ids }),
        )
        .await
}

/// The metadata of one statistic, or `None` when the recorder keeps none for it.
pub async fn statistic_metadata_for(
    connection: &Connection,
    statistic_id: &str,
) -> Result<Option<StatisticMetadata>, HaError> {
    let ids = [statistic_id.to_owned()];
    Ok(statistics_metadata(connection, &ids)
        .await?
        .into_iter()
        .next())
}

pub async fn list_statistic_ids(
    connection: &Connection,
    kind: Option<StatisticKind>,
) -> Result<Vec<StatisticMetadata>, HaError> {
    let params = match kind {
        Some(kind) => json!({ "statistic_type": kind }),
        None => json!({}),
    };
    connection.call("recorder/list_statistic_ids", params).await
}
