//! The `event` payload of one `subscribe_entities` frame.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::{CompressedDiff, CompressedState};

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct EntityDiffEvent {
    #[serde(default)]
    pub a: BTreeMap<String, CompressedState>,
    #[serde(default)]
    pub c: BTreeMap<String, CompressedDiff>,
    #[serde(default)]
    pub r: Vec<String>,
}
