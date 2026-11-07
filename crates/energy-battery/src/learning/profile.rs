//! The persisted learning profile: the numbers the planner uses, with their provenance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::learning::decision::LearningDecision;
use crate::learning::efficiency::EfficiencyEstimate;
use crate::learning::report::LearningReport;

/// One pack's durable learning result.
///
/// This is the row a `LearningStore` persists: the effective capacity, the split efficiency, the
/// confidence of each, and the decision that produced them. `capacity_kwh` is the *effective* value
/// (a carried-forward previous estimate is still in there); `capacity_applied` says whether that
/// value is a measurement rather than the typed seed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningProfile {
    /// Serial (or stable ledger id) of the pack.
    pub pack_id: String,
    /// Effective usable capacity, kWh; `None` until a measurement or a typed seed exists.
    pub capacity_kwh: Option<f64>,
    /// Confidence of `capacity_kwh` in `[0, 1]`.
    pub capacity_confidence: f64,
    /// Whether `capacity_kwh` comes from a measurement (fresh, clamped or carried forward).
    pub capacity_applied: bool,
    /// The last pass's decision for the capacity.
    pub capacity_decision: LearningDecision,
    /// Lower edge of the capacity band at the last pass.
    pub band_low_kwh: f64,
    /// Upper edge of the capacity band at the last pass.
    pub band_high_kwh: f64,
    /// AC-to-AC round trip in `(0, 1]`.
    pub round_trip: f64,
    /// `sqrt(round_trip)`, the charge leg.
    pub eta_charge: f64,
    /// `sqrt(round_trip)`, the discharge leg.
    pub eta_discharge: f64,
    /// Confidence of the round trip in `[0, 1]`.
    pub efficiency_confidence: f64,
    /// Whether the round trip is a measurement rather than the typed default.
    pub efficiency_applied: bool,
    /// Capacity samples that contributed at the last pass.
    pub sessions_used: usize,
    /// When the profile last changed.
    pub updated_at: DateTime<Utc>,
}

impl LearningProfile {
    /// The durable row for one learning pass.
    pub fn from_report(report: &LearningReport) -> Self {
        let capacity = &report.capacity;
        let efficiency = &report.efficiency;
        Self {
            pack_id: report.pack_id.clone(),
            capacity_kwh: capacity.applied_kwh,
            capacity_confidence: capacity.confidence,
            capacity_applied: capacity.decision.publishable() && capacity.applied_kwh.is_some(),
            capacity_decision: capacity.decision,
            band_low_kwh: capacity.band_low_kwh,
            band_high_kwh: capacity.band_high_kwh,
            round_trip: efficiency.round_trip,
            eta_charge: efficiency.eta_charge,
            eta_discharge: efficiency.eta_discharge,
            efficiency_confidence: efficiency.confidence,
            efficiency_applied: efficiency.is_confident(),
            sessions_used: report.used,
            updated_at: report.estimated_at,
        }
    }

    /// The capacity the planner should charge/discharge against, if there is one.
    pub fn effective_capacity_kwh(&self) -> Option<f64> {
        if self.capacity_applied {
            self.capacity_kwh
        } else {
            None
        }
    }

    /// The previous profile as an estimate, so the next pass can keep it on thin data.
    ///
    /// `pairs` and `pairs_refused` are unknown after a round trip through storage and stay 0; the
    /// kept-previous path only reads the numbers, the flags and the confidence.
    pub fn efficiency_estimate(&self) -> EfficiencyEstimate {
        EfficiencyEstimate {
            pack_id: self.pack_id.clone(),
            round_trip: self.round_trip,
            eta_charge: self.eta_charge,
            eta_discharge: self.eta_discharge,
            pairs: 0,
            pairs_refused: 0,
            confidence: self.efficiency_confidence,
            applied: self.efficiency_applied,
            kept_previous: false,
            measured_at: self.updated_at,
        }
    }
}
