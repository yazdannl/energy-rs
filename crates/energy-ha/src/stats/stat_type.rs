//! Requested statistics columns.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatType {
    Change,
    LastReset,
    Max,
    Mean,
    Min,
    State,
    Sum,
}
