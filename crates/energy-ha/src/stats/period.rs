//! Statistic bucket size accepted by the recorder.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Period {
    #[serde(rename = "5minute")]
    FiveMinute,
    #[serde(rename = "hour")]
    Hour,
    #[serde(rename = "day")]
    Day,
    #[serde(rename = "week")]
    Week,
    #[serde(rename = "month")]
    Month,
    #[serde(rename = "year")]
    Year,
}

impl Period {
    pub fn millis(self) -> i64 {
        match self {
            Self::FiveMinute => 5 * 60 * 1_000,
            Self::Hour => 60 * 60 * 1_000,
            Self::Day => 24 * 60 * 60 * 1_000,
            Self::Week => 7 * 24 * 60 * 60 * 1_000,
            Self::Month => 30 * 24 * 60 * 60 * 1_000,
            Self::Year => 365 * 24 * 60 * 60 * 1_000,
        }
    }
}
