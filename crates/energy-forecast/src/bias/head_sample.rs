//! One scored head sample: when it was for, its local hour, forecast and actual.

use chrono::{DateTime, Utc};

/// One archived forecast joined to its hourly mean measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadSample {
    /// The slot instant.
    pub moment: DateTime<Utc>,
    /// Local hour of day, 0..23.
    pub hour: u32,
    /// Forecast power, kW.
    pub forecast_kw: f64,
    /// Measured (or derived) power, kW.
    pub actual_kw: f64,
}
