//! One robust capacity prior shared by packs that carry the same label.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::learning::bounds::CapacityBounds;
use crate::learning::efficiency::EfficiencyEstimate;
use crate::learning::robust::weighted_median;
use crate::learning::sample::{CapacitySample, confidence};
use crate::learning::session::SessionRecord;

/// Cap on a pooled prior's confidence: AC energy and whole-percent SoC still share one systematic
/// efficiency error, however many identical packs agree.
const MAX_POOL_CONFIDENCE: f64 = 0.85;

/// Pooled evidence, keyed by label in milli-kWh so identical hardware lands in one bucket.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CapacityPool {
    groups: BTreeMap<i64, Pooled>,
}

/// One label's robust estimate and the evidence behind it.
#[derive(Debug, Clone, PartialEq)]
struct Pooled {
    estimate_kwh: f64,
    confidence: f64,
    packs: usize,
}

impl CapacityPool {
    /// Pool every batch's sessions with the others that share its label.
    ///
    /// Each pack contributes recency- and span-weighted samples through the same robust combiner the
    /// single-pack learner uses. A pack with many sessions cannot outvote a peer geometrically: the
    /// recency weights of any pack sum to roughly the same, so the median reflects the population.
    pub fn from_sessions(batches: &[(f64, &[SessionRecord])], now: DateTime<Utc>) -> Self {
        let mut grouped: BTreeMap<i64, (f64, Vec<CapacitySample>, usize)> = BTreeMap::new();
        for &(label_kwh, sessions) in batches {
            if !label_kwh.is_finite() || label_kwh <= 0.0 || sessions.is_empty() {
                continue;
            }
            let key = (label_kwh * 1000.0).round() as i64;
            let split =
                EfficiencyEstimate::from_sessions("", sessions, label_kwh, now, None).split();
            let entry = grouped.entry(key).or_insert((label_kwh, Vec::new(), 0));
            entry.2 += 1;
            entry.1.extend(
                sessions.iter().enumerate().filter_map(|(rank, session)| {
                    CapacitySample::from_session(session, split, rank)
                }),
            );
        }
        Self {
            groups: grouped
                .into_iter()
                .map(|(key, (label, samples, packs))| (key, Pooled::new(label, &samples, packs)))
                .collect(),
        }
    }

    /// The pooled `(capacity_kwh, confidence)` for a label, when peers have usable evidence.
    pub fn for_label(&self, label_kwh: f64) -> Option<(f64, f64)> {
        if !label_kwh.is_finite() || label_kwh <= 0.0 {
            return None;
        }
        self.groups
            .get(&((label_kwh * 1000.0).round() as i64))
            .map(|pooled| (pooled.estimate_kwh, pooled.confidence))
    }

    /// How many packs contributed to a label's pool.
    pub fn packs_for_label(&self, label_kwh: f64) -> usize {
        if !label_kwh.is_finite() || label_kwh <= 0.0 {
            return 0;
        }
        self.groups
            .get(&((label_kwh * 1000.0).round() as i64))
            .map_or(0, |pooled| pooled.packs)
    }
}

impl Pooled {
    /// Robustly combine the peers, then shrink the result toward the label before publishing it as
    /// a prior. A genuinely aged whole fleet can still move the pool: the shrinkage is partial.
    fn new(label_kwh: f64, samples: &[CapacitySample], packs: usize) -> Self {
        let pairs: Vec<(f64, f64)> = samples
            .iter()
            .map(|sample| (sample.capacity_kwh, sample.weight))
            .collect();
        let raw = weighted_median(&pairs).unwrap_or(label_kwh);
        let confidence = confidence(samples).min(MAX_POOL_CONFIDENCE);
        let bounds = CapacityBounds::new(label_kwh, None);
        let (low, high) = bounds.band();
        Self {
            estimate_kwh: (label_kwh + confidence * (raw - label_kwh)).clamp(low, high),
            confidence,
            packs,
        }
    }
}
