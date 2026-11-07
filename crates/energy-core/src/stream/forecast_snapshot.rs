//! The forecast totals behind the current plan.

/// Forecast energy for the remaining horizon, Wh, plus its confidence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ForecastSnapshot {
    /// When the forecast was refreshed, UTC epoch seconds.
    pub at: i64,
    /// First slot the totals cover, UTC epoch seconds.
    pub horizon_start: i64,
    /// Expected PV energy over the horizon, Wh.
    pub pv_wh: f64,
    /// Expected load energy over the horizon, Wh.
    pub load_wh: f64,
    /// Model confidence in `[0, 1]`, when the model reports one.
    pub confidence: Option<f64>,
}
