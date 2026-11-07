//! Request body for `history/history_during_period`.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::client::{Connection, HaError};

use super::HistoryResponse;

// The four booleans are the REST body's fields (`minimal_response`, `no_attributes`, ...), not a
// state machine: `true`/`false` is exactly how Home Assistant accepts them, so an enum would only
// rename the wire format.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistoryQuery {
    pub start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    pub entity_ids: Vec<String>,
    pub include_start_time_state: bool,
    pub significant_changes_only: bool,
    pub minimal_response: bool,
    pub no_attributes: bool,
}

impl HistoryQuery {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>, entity_ids: Vec<String>) -> Self {
        Self {
            start_time: start.to_rfc3339_opts(SecondsFormat::Secs, true),
            end_time: Some(end.to_rfc3339_opts(SecondsFormat::Secs, true)),
            entity_ids,
            include_start_time_state: true,
            significant_changes_only: false,
            minimal_response: true,
            no_attributes: true,
        }
    }
}

pub async fn history_during_period(
    connection: &Connection,
    query: &HistoryQuery,
) -> Result<HistoryResponse, HaError> {
    if query.entity_ids.is_empty() {
        return Ok(HistoryResponse::default());
    }
    connection
        .call(
            "history/history_during_period",
            serde_json::to_value(query)?,
        )
        .await
}
