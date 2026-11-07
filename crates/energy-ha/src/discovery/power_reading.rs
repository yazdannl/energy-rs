//! A numeric entity reading before normalization to W.

use crate::state::EntityState;

#[derive(Debug, Clone, PartialEq)]
pub struct PowerReading {
    pub value: f64,
    pub unit: String,
}

impl PowerReading {
    pub fn from_state(state: &EntityState) -> Option<Self> {
        Some(Self {
            value: state.numeric()?,
            unit: state.unit()?.to_owned(),
        })
    }

    pub fn watts(&self) -> Option<f64> {
        to_watts(self.value, &self.unit)
    }
}

/// Convert a power unit to W. Unknown units are missing, not guessed.
pub fn to_watts(value: f64, unit: &str) -> Option<f64> {
    match unit.trim() {
        "W" | "w" => Some(value),
        "kW" | "kw" => Some(value * 1_000.0),
        "MW" => Some(value * 1_000_000.0),
        "mW" => Some(value / 1_000.0),
        "µW" | "uW" => Some(value / 1_000_000.0),
        _ => None,
    }
}
