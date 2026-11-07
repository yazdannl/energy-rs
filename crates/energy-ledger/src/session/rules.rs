//! Rules that decide what counts as a session.

/// Extraction thresholds.
///
/// A sample belongs to a run when `|power_w| >= deadband_w`; a run needs at least `min_samples`
/// samples; a timestamp gap larger than `max_gap_intervals × interval` splits the input (the pieces
/// are then unbounded and therefore incomplete). Defaults are 20 W, 2 samples and 2 intervals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionRules {
    pub deadband_w: f64,
    pub min_samples: usize,
    pub max_gap_intervals: i64,
}

impl Default for SessionRules {
    fn default() -> Self {
        Self {
            deadband_w: 20.0,
            min_samples: 2,
            max_gap_intervals: 2,
        }
    }
}

impl SessionRules {
    pub fn new(deadband_w: f64, min_samples: usize, max_gap_intervals: i64) -> Self {
        Self {
            deadband_w,
            min_samples,
            max_gap_intervals,
        }
    }

    pub(crate) fn max_gap_s(self, interval_s: i64) -> i64 {
        self.max_gap_intervals.max(1) * interval_s
    }
}
