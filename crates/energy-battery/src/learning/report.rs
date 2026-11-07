//! One learning pass: efficiency, capacity, provenance and the reason it was applied.

use std::fmt;

use chrono::{DateTime, Utc};

use crate::control::Direction;
use crate::learning::bounds::CapacityBounds;
use crate::learning::capacity::{CapacityEstimate, MIN_SESSIONS};
use crate::learning::efficiency::EfficiencyEstimate;
use crate::learning::profile::LearningProfile;
use crate::learning::refusal::Refusal;
use crate::learning::session::SessionRecord;
use crate::learning::weight::{SessionWeight, recency_weight};

/// Everything one learning pass used, produced, refused and applied.
///
/// This is the object to log and to persist: it names the inputs (session count), the per-session
/// weights and sampled capacities, the estimate with its band and confidence, the efficiency, and
/// the decision that says whether the numbers were applied, clamped, or kept from before.
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
}

impl LearningReport {
    /// Learn one pack's capacity and efficiency from its sessions.
    ///
    /// The capacity basis is the charge side (what the pack accepts per SoC point) because a
    /// discharge drops a few more points than it gives back on a real BMS. Discharge sessions are
    /// the fallback only when fewer than [`MIN_SESSIONS`] charge sessions exist. The split
    /// efficiency from [`EfficiencyEstimate::from_sessions`] converts AC energy to SoC points; a
    /// pack with no measured cycles uses the typed default, and `applied` says so.
    pub fn learn(
        pack_id: &str,
        sessions: &[SessionRecord],
        bounds: &CapacityBounds,
        previous: Option<&LearningProfile>,
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
        let split = efficiency.split();
        let (accepted, refused) = accept(pack_id, sessions);
        let charges = collect(&accepted, Direction::Charge, split, false);
        let discharges = collect(&accepted, Direction::Discharge, split, true);
        let chosen = choose(charges, discharges);
        let samples: Vec<(f64, f64)> = chosen
            .iter()
            .map(|sample| (sample.capacity_kwh, sample.weight))
            .collect();
        let previous_capacity = previous.and_then(|profile| {
            profile
                .effective_capacity_kwh()
                .map(|value| (value, profile.capacity_confidence))
        });
        let capacity = CapacityEstimate::from_samples(&samples, bounds, previous_capacity);
        let weights = chosen
            .iter()
            .zip(capacity.kept.iter().copied())
            .map(|(sample, kept)| SessionWeight {
                started_at: sample.started_at,
                direction: sample.direction,
                rank: sample.rank,
                weight: sample.weight,
                capacity_kwh: sample.capacity_kwh,
                fallback: sample.fallback,
                kept,
            })
            .collect();
        Self {
            pack_id: pack_id.to_owned(),
            inputs: sessions.len(),
            used: capacity.used,
            weights,
            capacity,
            efficiency,
            refused,
            estimated_at: now,
        }
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
        format!(
            "pack {}: capacity {capacity} (raw {raw}, band {:.3}..{:.3}, {}), efficiency {:.4} ({}), used {}/{} sessions, refused {}",
            self.pack_id,
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
    capacity_kwh: f64,
    weight: f64,
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

/// Turn one direction's sessions into recency-weighted capacity samples.
///
/// `C = I · x / dSOC` while charging (the pack accepts `I · x` into the cells) and
/// `C = O / (x · dSOC)` while discharging (it delivers `C · x` at the terminals), with `dSOC` as a
/// fraction and `x` the split efficiency both legs share.
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
        let span = session.soc_span_pct() / 100.0;
        let capacity_kwh = match direction {
            Direction::Charge => session.energy_ac_kwh * split / span,
            Direction::Discharge => session.energy_ac_kwh / split / span,
        };
        if span > 0.0 && capacity_kwh.is_finite() && capacity_kwh > 0.0 {
            samples.push(Sample {
                started_at: session.started_at,
                direction,
                rank,
                capacity_kwh,
                weight: recency_weight(rank),
                fallback,
            });
        }
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
