//! One entity's compressed change (`+` changes, `-` removed attribute keys).

use serde::Deserialize;

use super::CompressedPatch;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct CompressedDiff {
    #[serde(rename = "+", default)]
    pub plus: Option<CompressedPatch>,
    #[serde(rename = "-", default)]
    pub minus: Option<Vec<String>>,
}
