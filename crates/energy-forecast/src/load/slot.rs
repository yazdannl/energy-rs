//! One quarter-hour load slot.

use chrono::{DateTime, Utc};

/// One quarter-hour load slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadSlot {
    /// Slot start, UTC.
    pub t: DateTime<Utc>,
    /// Point forecast, kW.
    pub load_kw: f64,
    /// Lower band, kW.
    pub p10_kw: f64,
    /// Upper band, kW.
    pub p90_kw: f64,
}
