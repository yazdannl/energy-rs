//! Timestamped evidence that makes an applied-watts number publishable.

/// A real flow sample: the timestamp is what separates measurement from an accepted echo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredWatts {
    pub(crate) watts: f64,
    #[allow(dead_code)]
    pub(crate) at: i64,
}

impl MeasuredWatts {
    /// Stamp a real sample.
    #[must_use]
    pub const fn new(watts: f64, at: i64) -> Self {
        Self { watts, at }
    }
}
