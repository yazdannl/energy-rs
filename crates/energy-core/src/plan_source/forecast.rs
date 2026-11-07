//! One forward forecast: the two 15-minute series the solver consumes.

use crate::plan_source::ForecastQuality;

/// PV and load for the horizon, in kW per slot.
///
/// The series are aligned to the quarter-hour slot containing the plan instant; the plan source
/// slices them to the horizon the price book allows. `as_of` is the capture instant (epoch seconds)
/// and `note` explains a degraded series in words the UI can show.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// PV production, kW per slot.
    pub pv_kw: Vec<f64>,
    /// House load, kW per slot.
    pub load_kw: Vec<f64>,
    /// Capture instant, UTC epoch seconds.
    pub as_of: i64,
    /// Whether the series came from a provider/model or a fallback.
    pub quality: ForecastQuality,
    /// Why the forecast is degraded, when it is.
    pub note: Option<String>,
}

impl Forecast {
    /// A fresh forecast.
    pub fn fresh(pv_kw: Vec<f64>, load_kw: Vec<f64>, as_of: i64) -> Self {
        Self {
            pv_kw,
            load_kw,
            as_of,
            quality: ForecastQuality::Fresh,
            note: None,
        }
    }

    /// A fallback forecast with the reason it was used.
    pub fn degraded(
        pv_kw: Vec<f64>,
        load_kw: Vec<f64>,
        as_of: i64,
        note: impl Into<String>,
    ) -> Self {
        Self {
            pv_kw,
            load_kw,
            as_of,
            quality: ForecastQuality::Degraded,
            note: Some(note.into()),
        }
    }

    /// The number of slots both series cover.
    pub fn len(&self) -> usize {
        self.pv_kw.len().min(self.load_kw.len())
    }

    /// Whether either series is empty.
    pub fn is_empty(&self) -> bool {
        self.pv_kw.is_empty() || self.load_kw.is_empty()
    }

    /// Whether both series are finite, which is what the builder requires.
    pub fn is_finite(&self) -> bool {
        self.pv_kw
            .iter()
            .chain(&self.load_kw)
            .all(|value| value.is_finite())
    }
}
