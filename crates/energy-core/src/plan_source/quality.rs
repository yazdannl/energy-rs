//! How trustworthy a forward forecast is.

/// Whether a forecast is a model/provider output or a marked fallback.
///
/// A degraded forecast is still planned with -- holding for a whole day because a provider is down
/// is worse than planning on the last days' shape -- but the plan summary says so and the sell
/// floor still comes from the prices, never from the guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForecastQuality {
    /// A provider or model produced the series.
    Fresh,
    /// A fallback (EWMA of the measured history) produced the series.
    Degraded,
}

impl ForecastQuality {
    /// Whether this is a fallback.
    pub const fn is_degraded(self) -> bool {
        matches!(self, Self::Degraded)
    }

    /// Stable slug for logs and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Degraded => "degraded",
        }
    }
}
