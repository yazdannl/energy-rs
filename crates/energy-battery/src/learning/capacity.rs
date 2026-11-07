//! Per-pack capacity from AC energy per SoC point.

use crate::learning::bounds::CapacityBounds;
use crate::learning::decision::LearningDecision;
use crate::learning::robust::{mad_inliers, weighted_median};

/// Usable sessions needed before an estimate may be published.
pub const MIN_SESSIONS: usize = 3;
/// Median-absolute-deviation limit for the outlier trim (in sigmas).
pub const MAD_LIMIT: f64 = 3.5;
/// A home pack below 1 kWh is a toy or a measurement error.
pub const MIN_PLAUSIBLE_KWH: f64 = 1.0;
/// Above this the "capacity" is an accumulated counter, not a pack.
pub const MAX_PLAUSIBLE_KWH: f64 = 200.0;

/// The learned capacity, its band, and the reason it was or was not published.
///
/// `raw_kwh` is what the sessions said; `applied_kwh` is what may be used by the planner (the raw
/// value clamped into [`CapacityBounds`], or the previous published value when this pass has too
/// little data). `kept` is aligned with the samples handed in, so the report can show per-session
/// provenance instead of only a total.
#[derive(Debug, Clone, PartialEq)]
pub struct CapacityEstimate {
    /// Weighted median of the accepted samples before the band clamp.
    pub raw_kwh: Option<f64>,
    /// The value to publish, if any: clamped, or the previous value when `KeptPrevious`.
    pub applied_kwh: Option<f64>,
    /// Lower edge of the publishing band.
    pub band_low_kwh: f64,
    /// Upper edge: never above the reported capacity.
    pub band_high_kwh: f64,
    /// Confidence in `[0, 1]`, from the number of samples that survived the trim.
    pub confidence: f64,
    /// Whether *this* pass published a fresh measurement.
    pub applied: bool,
    /// Why this pass ended where it did.
    pub decision: LearningDecision,
    /// Samples that survived the plausibility envelope and the MAD trim.
    pub used: usize,
    /// Samples the MAD trim dropped (implausible samples never reach `used` either).
    pub trimmed: usize,
    /// Which input samples survived, aligned with the input slice.
    pub kept: Vec<bool>,
}

impl CapacityEstimate {
    /// Combine `(capacity_kwh, weight)` samples against the pack's bounds.
    ///
    /// The pipeline is: drop non-finite, implausible or non-positively-weighted samples, trim
    /// outliers with a median-absolute-deviation filter (a zero MAD keeps everything), take the
    /// weighted median, then clamp into the band. Fewer than [`MIN_SESSIONS`] survivors do not
    /// publish: a previous `(value, confidence)` is carried forward (`KeptPrevious`) and nothing
    /// else happens, so a thin day cannot wipe a measured capacity or its confidence.
    pub fn from_samples(
        samples: &[(f64, f64)],
        bounds: &CapacityBounds,
        previous: Option<(f64, f64)>,
    ) -> Self {
        let (band_low_kwh, band_high_kwh) = bounds.band();
        let (usable, values) = usable_samples(samples);
        let inliers = mad_inliers(&values, MAD_LIMIT);
        let mut kept = vec![false; samples.len()];
        let mut kept_pairs = Vec::with_capacity(usable.len());
        for (position, &index) in usable.iter().enumerate() {
            if inliers[position] {
                kept[index] = true;
                kept_pairs.push(samples[index]);
            }
        }
        let used = kept_pairs.len();
        let trimmed = usable.len() - used;
        let raw_kwh = weighted_median(&kept_pairs);
        let mut estimate = Self {
            raw_kwh,
            applied_kwh: None,
            band_low_kwh,
            band_high_kwh,
            confidence: 0.0,
            applied: false,
            decision: LearningDecision::NoSessions,
            used,
            trimmed,
            kept,
        };
        if used >= MIN_SESSIONS && bounds.is_usable() {
            estimate.publish();
            return estimate;
        }
        match (previous, usable.is_empty()) {
            (Some((value, confidence)), _) => {
                estimate.decision = LearningDecision::KeptPrevious;
                estimate.applied_kwh = Some(value);
                estimate.confidence = confidence;
            }
            (None, false) => estimate.decision = LearningDecision::LowConfidence,
            (None, true) => estimate.decision = LearningDecision::NoSessions,
        }
        estimate
    }

    /// Clamp the raw estimate into the band and record which edge moved it.
    fn publish(&mut self) {
        let Some(raw) = self.raw_kwh else {
            self.decision = LearningDecision::NoSessions;
            return;
        };
        let clamped = raw.clamp(self.band_low_kwh, self.band_high_kwh);
        self.decision = if raw > self.band_high_kwh {
            LearningDecision::ClampedHigh
        } else if raw < self.band_low_kwh {
            LearningDecision::ClampedLow
        } else {
            LearningDecision::Applied
        };
        self.applied = true;
        self.applied_kwh = Some(clamped);
        self.confidence = confidence(self.used);
    }
}

/// The indices and values of samples worth combining.
fn usable_samples(samples: &[(f64, f64)]) -> (Vec<usize>, Vec<f64>) {
    let mut indices = Vec::new();
    let mut values = Vec::new();
    for (index, (value, weight)) in samples.iter().enumerate() {
        let usable = value.is_finite()
            && weight.is_finite()
            && *weight > 0.0
            && (MIN_PLAUSIBLE_KWH..=MAX_PLAUSIBLE_KWH).contains(value);
        if usable {
            indices.push(index);
            values.push(*value);
        }
    }
    (indices, values)
}

/// Confidence rises with the survivor count and reaches 1 at twice [`MIN_SESSIONS`].
fn confidence(used: usize) -> f64 {
    (used as f64 / (2.0 * MIN_SESSIONS as f64)).min(1.0)
}
