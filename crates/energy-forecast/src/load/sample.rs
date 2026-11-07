//! One hourly load sample.

use chrono::{DateTime, Utc};

/// One hourly house-load reading in kW, keyed by its UTC hour start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadSample {
    /// Hour start, UTC.
    pub moment: DateTime<Utc>,
    /// Average load over the hour, kW.
    pub load_kw: f64,
}
