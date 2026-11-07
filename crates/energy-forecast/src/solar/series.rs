//! A timestamped kW series with the provider's own interpolation rules.

use chrono::{DateTime, Utc};

/// One source's PV series, sorted by timestamp.
///
/// The asymmetry in [`Series::interpolate`] is part of the contract: 0 before the first sample and
/// the **last value** after the last one, because a source with a short horizon repeats its last
/// value rather than decaying to zero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Series {
    samples: Vec<(DateTime<Utc>, f64)>,
}

impl Series {
    /// Wrap samples, sorting by timestamp and dropping non-finite values.
    pub fn new(mut samples: Vec<(DateTime<Utc>, f64)>) -> Self {
        samples.retain(|(_, value)| value.is_finite());
        samples.sort_by_key(|(stamp, _)| *stamp);
        Self { samples }
    }

    /// Build from a map keyed by timestamp.
    pub fn from_map(values: &std::collections::BTreeMap<DateTime<Utc>, f64>) -> Self {
        Self::new(
            values
                .iter()
                .map(|(stamp, value)| (*stamp, *value))
                .collect(),
        )
    }

    /// The samples, in timestamp order.
    pub fn samples(&self) -> &[(DateTime<Utc>, f64)] {
        &self.samples
    }

    /// Whether the series has no samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Number of samples.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Linear interpolation at `moment`, with the provider's edge rules.
    pub fn interpolate(&self, moment: DateTime<Utc>) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let position = self.samples.partition_point(|(stamp, _)| *stamp < moment);
        if position == 0 {
            let (first_stamp, first_value) = self.samples[0];
            return if first_stamp > moment {
                0.0
            } else {
                first_value
            };
        }
        if position >= self.samples.len() {
            return self.samples[self.samples.len() - 1].1;
        }
        let (t0, v0) = self.samples[position - 1];
        let (t1, v1) = self.samples[position];
        if t1 == t0 {
            return v1;
        }
        let fraction =
            (moment - t0).num_milliseconds() as f64 / (t1 - t0).num_milliseconds() as f64;
        v0 + (v1 - v0) * fraction
    }
}
