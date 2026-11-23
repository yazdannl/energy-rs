//! One learning pass: efficiency, capacity, provenance and the reason it was applied.

use std::fmt;

use chrono::{DateTime, Utc};

use crate::control::Direction;
use crate::learning::bounds::CapacityBounds;
use crate::learning::capacity::{CapacityEstimate, MIN_SESSIONS};
use crate::learning::efficiency::EfficiencyEstimate;
use crate::learning::pool::CapacityPool;
use crate::learning::profile::{LearningProfile, NEVER_MEASURED_AT};
use crate::learning::refusal::Refusal;
use crate::learning::sample::{CapacitySample, span_quality};
use crate::learning::session::SessionRecord;
use crate::learning::weight::{SessionWeight, recency_weight};

/// Everything one learning pass used, produced, refused and applied.
///
/// This is the object to log and to persist: it names the inputs (session count), the per-session
/// weights and sampled capacities, the estimate with its band, evidence and confidence, the
/// efficiency, and the decision that says whether the numbers were applied, clamped or kept.
#[derive(Debug, Clone, PartialEq)]
pub struct LearningReport {
    /// Serial (or stable ledger id) of the pack.
    pub pack_id: String,
    /// Sessions handed to the learner.
    pub inputs: usize,
    /// Samples that contributed to the applied capacity.
    pub used: usize,
    /// One row per accepted session, in the basis actually used.
    pub weights: Vec<SessionWeight>,
    /// The capacity estimate with its band and decision.
    pub capacity: CapacityEstimate,
    /// The round-trip estimate from the same sessions.
    pub efficiency: EfficiencyEstimate,
    /// Sessions the pass refused, with the reason.
    pub refused: Vec<Refusal>,
    /// When the pass ran.
    pub estimated_at: DateTime<Utc>,
    /// When a real measurement last landed: now on a measured pass, otherwise the previous value.
    pub measured_at: DateTime<Utc>,
}

impl LearningReport {
    /// Learn one pack's capacity and efficiency from its sessions, without peer evidence.
    pub fn learn(
        pack_id: &str,
        sessions: &[SessionRecord],
        bounds: &CapacityBounds,
        previous: Option<&LearningProfile>,
        now: DateTime<Utc>,
    ) -> Self {
        Self::learn_with_pool(pack_id, sessions, bounds, previous, None, now)
    }

    /// Learn one pack, using the identical-label peer pool as the capacity prior.
    ///
    /// The capacity basis is the charge side (what the pack accepts per SoC point) because a
    /// discharge drops a few more points than it gives back on a real BMS. Discharge sessions are
    /// the fallback only when fewer than [`MIN_SESSIONS`] charge sessions exist. The split
    /// efficiency from [`EfficiencyEstimate::from_sessions`] converts AC energy to SoC points; a
    /// pack with no measured cycles uses the typed default, and `applied` says so.
    pub fn learn_with_pool(
        pack_id: &str,
        sessions: &[SessionRecord],
        bounds: &CapacityBounds,
        previous: Option<&LearningProfile>,
        pool: Option<&CapacityPool>,
        now: DateTime<Utc>,
    ) -> Self {
        let previous_efficiency = previous.map(LearningProfile::efficiency_estimate);
        let efficiency = EfficiencyEstimate::from_sessions(
            pack_id,
            sessions,
            bounds.label_kwh,
            now,
            previous_efficiency.as_ref(),
        );
        let (accepted, refused) = accept(pack_id, sessions);
        let charges = collect(&accepted, Direction::Charge, efficiency.split(), false);
        let discharges = collect(&accepted, Direction::Discharge, efficiency.split(), true);
        let chosen = choose(charges, discharges);
        let samples: Vec<CapacitySample> = chosen.iter().map(|sample| sample.measurement).collect();
        let previous_capacity = previous.and_then(|profile| {
            profile
                .effective_capacity_kwh()
                .map(|value| (value, profile.capacity_confidence))
        });
        let prior = pool.and_then(|pool| pool.for_label(bounds.label_kwh));
        let capacity = CapacityEstimate::from_evidence(&samples, bounds, previous_capacity, prior);
        let weights = chosen
            .iter()
            .zip(capacity.kept.iter().copied())
            .map(|(sample, kept)| SessionWeight {
                started_at: sample.started_at,
                direction: sample.direction,
                rank: sample.rank,
                weight: recency_weight(sample.rank),
                span_quality: span_quality(sample.measurement.soc_points),
                soc_points: sample.measurement.soc_points,
                capacity_kwh: sample.measurement.capacity_kwh,
                fallback: sample.fallback,
                kept,
            })
            .collect();
        let measured = capacity.decision.applied() || efficiency.applied || efficiency.pairs > 0;
        let measured_at = if measured {
            now
        } else {
            previous.map_or(NEVER_MEASURED_AT, |profile| profile.updated_at)
        };
        Self {
            pack_id: pack_id.to_owned(),
            inputs: sessions.len(),
            used: capacity.used,
            weights,
            capacity,
            efficiency,
            refused,
            estimated_at: now,
            measured_at,
        }
    }

    /// Whether this pass published a capacity or efficiency measurement.
    pub const fn measured(&self) -> bool {
        self.capacity.decision.applied() || self.efficiency.applied || self.efficiency.pairs > 0
    }

    /// One line for the log: what was used, what was published, and why.
    pub fn summary(&self) -> String {
        let capacity = self.capacity.applied_kwh.map_or_else(
            || "no estimate".to_owned(),
            |value| format!("{value:.4} kWh"),
        );
        let raw = self
            .capacity
            .raw_kwh
            .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.4}"));
        let pairing = self.efficiency.diagnostics.summary();
        format!(
            "pack {}: capacity {capacity} (raw {raw}, source {}, confidence {:.3}, {} points/{} cycles, band {:.3}..{:.3}, {}), efficiency {:.4} ({}), used {}/{} sessions, refused {}, pairing [{pairing}]",
            self.pack_id,
            self.capacity.source.slug(),
            self.capacity.confidence,
            self.capacity.soc_points,
            self.capacity.cycles_used,
            self.capacity.band_low_kwh,
            self.capacity.band_high_kwh,
            self.capacity.decision.slug(),
            self.efficiency.round_trip,
            self.efficiency.slug(),
            self.used,
            self.inputs,
            self.refused.len(),
        )
    }
}

impl fmt::Display for LearningReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.summary())
    }
}

/// One accepted session, ready to become a capacity sample.
struct Sample {
    started_at: DateTime<Utc>,
    direction: Direction,
    rank: usize,
    measurement: CapacitySample,
    fallback: bool,
}

/// Split the input into accepted sessions and refusals, keeping pack identity honest.
fn accept<'a>(
    pack_id: &str,
    sessions: &'a [SessionRecord],
) -> (Vec<&'a SessionRecord>, Vec<Refusal>) {
    let mut accepted = Vec::new();
    let mut refused = Vec::new();
    for session in sessions {
        if session.pack_id != pack_id {
            refused.push(Refusal::ForeignPack {
                at: session.started_at,
                pack_id: session.pack_id.clone(),
            });
            continue;
        }
        match session.refusal() {
            Some(reason) => refused.push(reason),
            None => accepted.push(session),
        }
    }
    (accepted, refused)
}

/// Turn one direction's sessions into recency- and span-weighted capacity samples.
fn collect(
    sessions: &[&SessionRecord],
    direction: Direction,
    split: f64,
    fallback: bool,
) -> Vec<Sample> {
    let mut selected: Vec<&SessionRecord> = sessions
        .iter()
        .copied()
        .filter(|session| session.direction == direction)
        .collect();
    selected.sort_by_key(|session| std::cmp::Reverse(session.started_at));
    let mut samples = Vec::with_capacity(selected.len());
    for (rank, session) in selected.iter().enumerate() {
        let Some(sample) = CapacitySample::from_session(session, split, rank) else {
            continue;
        };
        samples.push(Sample {
            started_at: session.started_at,
            direction,
            rank,
            measurement: sample,
            fallback,
        });
    }
    samples
}

/// The charge basis when it has enough sessions, the discharge fallback otherwise.
fn choose(charges: Vec<Sample>, discharges: Vec<Sample>) -> Vec<Sample> {
    if charges.len() >= MIN_SESSIONS || discharges.is_empty() {
        charges
    } else {
        discharges
    }
}
