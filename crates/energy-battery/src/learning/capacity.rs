//! Per-pack capacity: robust measurement, honest confidence, shrinkage to the label or peer pool.

use crate::learning::bounds::{AGING_CONFIDENCE, CapacityBounds};
use crate::learning::decision::LearningDecision;
use crate::learning::robust::{mad_inliers, weighted_median};
use crate::learning::sample::{CapacitySample, confidence};
use crate::learning::source::CapacitySource;

/// Usable sessions needed before an estimate may be published.
pub const MIN_SESSIONS: usize = 3;
/// Median-absolute-deviation limit for the outlier trim (in sigmas).
pub const MAD_LIMIT: f64 = 3.5;
/// A home pack below 1 kWh is a toy or a measurement error.
pub const MIN_PLAUSIBLE_KWH: f64 = 1.0;
/// Above this the "capacity" is an accumulated counter, not a pack.
pub const MAX_PLAUSIBLE_KWH: f64 = 200.0;
/// A peer pool needs this much confidence before it may replace the label as a prior.
pub const MIN_PRIOR_CONFIDENCE: f64 = 0.20;
/// At this confidence a pack's own long-span evidence owns the published number.
pub const OWN_SOURCE_CONFIDENCE: f64 = 0.50;
/// Span assumed by the legacy `(capacity, weight)` entry point.
const LEGACY_SPAN_PCT: f64 = 30.0;

/// The learned capacity, its band, evidence and the reason it was or was not published.
///
/// `raw_kwh` is the robust measurement; `applied_kwh` is the posterior — the prior moved toward the
/// measurement by the confidence the SoC coverage earns, then clamped into the band. `kept` is
/// aligned with the samples handed in, so the report can show per-session provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct CapacityEstimate {
    /// Weighted median of the accepted samples before shrinkage or the band clamp.
    pub raw_kwh: Option<f64>,
    /// The value to publish, if any: shrunk, clamped, or the previous value when `KeptPrevious`.
    pub applied_kwh: Option<f64>,
    /// Lower edge of the publishing band.
    pub band_low_kwh: f64,
    /// Upper edge: never above the reported capacity.
    pub band_high_kwh: f64,
    /// Confidence from covered SoC and span length, not from session count.
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
    /// Total SoC points covered by the surviving samples.
    pub soc_points: f64,
    /// Surviving charge/discharge cycles behind this estimate.
    pub cycles_used: usize,
    /// Whether the published value came from the label, the peer pool or this pack alone.
    pub source: CapacitySource,
}

impl CapacityEstimate {
    /// Combine capacity samples against the pack's bounds and prior.
    ///
    /// Non-finite, implausible or non-positively-weighted samples are dropped, outliers are trimmed
    /// by a median-absolute-deviation filter (a zero MAD keeps everything), and a weighted median
    /// gives a robust raw measurement. Fewer than [`MIN_SESSIONS`] survivors carry the previous
    /// value forward instead of publishing. The prior is the peer pool when one exists, otherwise
    /// the label or the pack's own reported capacity.
    pub fn from_evidence(
        samples: &[CapacitySample],
        bounds: &CapacityBounds,
        previous: Option<(f64, f64)>,
        prior: Option<(f64, f64)>,
    ) -> Self {
        let (band_low_kwh, band_high_kwh) = bounds.band();
        let mut estimate = Self {
            raw_kwh: None,
            applied_kwh: None,
            band_low_kwh,
            band_high_kwh,
            confidence: 0.0,
            applied: false,
            decision: LearningDecision::NoSessions,
            used: 0,
            trimmed: 0,
            kept: vec![false; samples.len()],
            soc_points: 0.0,
            cycles_used: 0,
            source: CapacitySource::Label,
        };
        let (usable, values) = usable_samples(samples);
        let inliers = mad_inliers(&values, MAD_LIMIT);
        let mut survivors = Vec::with_capacity(usable.len());
        for (position, &index) in usable.iter().enumerate() {
            if inliers[position] {
                estimate.kept[index] = true;
                survivors.push(samples[index]);
            }
        }
        estimate.trimmed = usable.len() - survivors.len();
        estimate.used = survivors.len();
        estimate.cycles_used = survivors.len();
        estimate.soc_points = survivors.iter().map(|sample| sample.soc_points).sum();
        estimate.raw_kwh = weighted_median(
            &survivors
                .iter()
                .map(|s| (s.capacity_kwh, s.weight))
                .collect::<Vec<_>>(),
        );
        if estimate.used < MIN_SESSIONS || !bounds.is_usable() {
            estimate.carry(previous, usable.is_empty());
            return estimate;
        }
        let confidence = confidence(&survivors);
        if confidence >= AGING_CONFIDENCE {
            (estimate.band_low_kwh, estimate.band_high_kwh) = bounds.aging_band();
        }
        let prior = prior.filter(|(_, confidence)| *confidence >= MIN_PRIOR_CONFIDENCE);
        estimate.publish(prior, bounds.prior_kwh(), confidence);
        estimate
    }

    /// Legacy entry point for callers that have only `(capacity, weight)` pairs.
    pub fn from_samples(
        samples: &[(f64, f64)],
        bounds: &CapacityBounds,
        previous: Option<(f64, f64)>,
    ) -> Self {
        let expanded: Vec<CapacitySample> = samples
            .iter()
            .map(|&(capacity_kwh, weight)| CapacitySample {
                capacity_kwh,
                weight,
                soc_points: LEGACY_SPAN_PCT,
            })
            .collect();
        Self::from_evidence(&expanded, bounds, previous, None)
    }

    /// Thin passes keep the previous number instead of resetting to the label.
    fn carry(&mut self, previous: Option<(f64, f64)>, usable_empty: bool) {
        match previous {
            Some((value, confidence)) => {
                self.decision = LearningDecision::KeptPrevious;
                self.applied_kwh = Some(value);
                self.confidence = confidence;
                self.source = CapacitySource::Previous;
            }
            None if usable_empty => self.decision = LearningDecision::NoSessions,
            None => self.decision = LearningDecision::LowConfidence,
        }
    }

    /// Shrink the raw measurement toward the prior, clamp, and record provenance.
    fn publish(&mut self, prior: Option<(f64, f64)>, label_prior: f64, confidence: f64) {
        let Some(raw) = self.raw_kwh else {
            self.decision = LearningDecision::NoSessions;
            return;
        };
        let pooled = prior.is_some();
        let prior_value = prior.map_or(label_prior, |(value, _)| value);
        let prior_value =
            prior_value.clamp(self.band_low_kwh, self.band_high_kwh.max(self.band_low_kwh));
        let posterior = prior_value + confidence * (raw - prior_value);
        self.decision = if posterior > self.band_high_kwh {
            LearningDecision::ClampedHigh
        } else if posterior < self.band_low_kwh {
            LearningDecision::ClampedLow
        } else {
            LearningDecision::Applied
        };
        self.applied = true;
        self.applied_kwh = Some(posterior.clamp(self.band_low_kwh, self.band_high_kwh));
        self.confidence = confidence;
        self.source = if confidence >= OWN_SOURCE_CONFIDENCE {
            CapacitySource::Own
        } else if pooled {
            CapacitySource::Pooled
        } else {
            CapacitySource::Label
        };
    }
}

/// The indices and values of samples worth combining.
fn usable_samples(samples: &[CapacitySample]) -> (Vec<usize>, Vec<f64>) {
    let mut indices = Vec::new();
    let mut values = Vec::new();
    for (index, sample) in samples.iter().enumerate() {
        let usable = sample.capacity_kwh.is_finite()
            && sample.weight.is_finite()
            && sample.weight > 0.0
            && sample.soc_points.is_finite()
            && sample.soc_points > 0.0
            && (MIN_PLAUSIBLE_KWH..=MAX_PLAUSIBLE_KWH).contains(&sample.capacity_kwh);
        if usable {
            indices.push(index);
            values.push(sample.capacity_kwh);
        }
    }
    (indices, values)
}
