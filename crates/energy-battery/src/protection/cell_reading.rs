//! One pack's cell voltage as the guard sees it.

use chrono::{DateTime, Utc};

/// A cell-voltage reading with the instant it was taken.
///
/// The timestamp is part of the reading, not of the call, so a cached report can be handed in
/// unchanged and the guard itself decides whether it is still evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellReading {
    /// The lowest cell voltage, V; `None` when the pack did not report one.
    pub voltage_v: Option<f64>,
    /// When the reading was taken.
    pub at: DateTime<Utc>,
}

impl CellReading {
    /// A reading the pack actually reported.
    pub const fn fresh(voltage_v: f64, at: DateTime<Utc>) -> Self {
        Self {
            voltage_v: Some(voltage_v),
            at,
        }
    }

    /// A poll that returned no cell voltage at all.
    pub const fn missing(at: DateTime<Utc>) -> Self {
        Self {
            voltage_v: None,
            at,
        }
    }

    /// Whether the value is a real, positive voltage.
    pub fn is_usable(&self) -> bool {
        self.voltage_v
            .is_some_and(|voltage| voltage.is_finite() && voltage > 0.0)
    }

    /// Whether this reading may change the latch.
    ///
    /// A future timestamp (clock skew between the pack and the add-on) is not stale: the reading is
    /// newer than "now", and treating it as old would drop a real measurement.
    pub fn is_fresh(&self, now: DateTime<Utc>, stale_after_s: f64) -> bool {
        if !self.is_usable() || !stale_after_s.is_finite() || stale_after_s < 0.0 {
            return false;
        }
        let age_s = (now - self.at).num_milliseconds() as f64 / 1000.0;
        age_s <= stale_after_s
    }
}
