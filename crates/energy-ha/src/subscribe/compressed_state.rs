//! Compressed state from the initial `a` snapshot.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::state::EntityState;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct CompressedState {
    #[serde(default)]
    pub s: Value,
    #[serde(default)]
    pub a: Option<Map<String, Value>>,
    #[serde(default)]
    pub c: Option<Value>,
    #[serde(default)]
    pub lc: Option<f64>,
    #[serde(default)]
    pub lu: Option<f64>,
}

impl CompressedState {
    pub(crate) fn to_entity(&self, entity_id: &str) -> EntityState {
        EntityState {
            entity_id: entity_id.to_owned(),
            state: self.s.clone(),
            attributes: self.a.clone().unwrap_or_default(),
            last_changed: self.lc,
            last_updated: self.lu.or(self.lc),
        }
    }
}
