//! One configured source: identity, weight, state, series and its own correction.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::kind::SourceKind;
use super::series::Series;
use super::state::SourceState;
use crate::bias::Correction;
use crate::time::local_hour;

/// One PV forecast source, ready for the merge.
///
/// Each source carries its own correction: the EWMA/head factors learned for that provider's
/// forecast are not transferable to another provider's bias.
#[derive(Debug, Clone)]
pub struct SolarSource {
    /// Stable, unique display name.
    pub name: String,
    /// Which provider produced it.
    pub kind: SourceKind,
    /// Merge weight; finite and `> 0`.
    pub weight: f64,
    /// The state the provider reports.
    pub state: SourceState,
    /// A note for the status detail.
    pub note: String,
    /// The source's kW series.
    pub series: Series,
    /// The source's own bias correction.
    pub correction: Correction,
}

impl SolarSource {
    /// A source with no correction and no note.
    pub fn new(
        name: impl Into<String>,
        kind: SourceKind,
        weight: f64,
        state: SourceState,
        series: Series,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            weight,
            state,
            note: String::new(),
            series,
            correction: Correction::neutral(),
        }
    }

    /// Attach a status note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }

    /// Attach the source's own bias correction.
    #[must_use]
    pub fn with_correction(mut self, correction: Correction) -> Self {
        self.correction = correction;
        self
    }

    /// Whether this source contributes to the merge.
    ///
    /// A disabled source and a source with an empty series are skipped. A non-empty series still
    /// contributes even when its status is `failed`; only the report row says `failed`.
    pub fn contributes(&self) -> bool {
        self.state != SourceState::Disabled && !self.series.is_empty()
    }

    /// The corrected kW at one slot: interpolated, multiplied by the correction, floored at zero.
    pub fn value_at(&self, moment: DateTime<Utc>, zone: Tz, now: DateTime<Utc>) -> f64 {
        let raw = self.series.interpolate(moment);
        self.correction.apply(raw, local_hour(moment, zone), now)
    }

    /// Weight used by the merge; a non-finite or non-positive weight is replaced by 1.0.
    pub fn safe_weight(&self) -> f64 {
        if self.weight.is_finite() && self.weight > 0.0 {
            self.weight
        } else {
            1.0
        }
    }
}
