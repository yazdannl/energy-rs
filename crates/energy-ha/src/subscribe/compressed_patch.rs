//! Fields changed in a compressed diff (`+`).

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct CompressedPatch {
    /// Present state, including an explicit `null` (hence the custom deserializer).
    #[serde(default, deserialize_with = "deserialize_present")]
    pub s: Option<Value>,
    /// Added/changed attributes (merged, not replaced).
    #[serde(default)]
    pub a: Option<Map<String, Value>>,
    #[serde(default)]
    pub c: Option<Value>,
    #[serde(default)]
    pub lc: Option<f64>,
    #[serde(default)]
    pub lu: Option<f64>,
}

/// `null` in a patch means "state is now null", not "field missing".
fn deserialize_present<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Value::deserialize(deserializer)?))
}
