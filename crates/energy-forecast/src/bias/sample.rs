//! One scored bias sample: what was forecast, what happened.

use chrono::{DateTime, Utc};

/// One `(timestamp, forecast_kw, measured_kw)` row folded into the hourly EWMA.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiasSample {
    /// The instant the forecast was for.
    pub moment: DateTime<Utc>,
    /// Forecast PV power, kW.
    pub forecast_kw: f64,
    /// Measured PV power, kW.
    pub measured_kw: f64,
}
