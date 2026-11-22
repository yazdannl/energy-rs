//! Round-trip efficiency from closed charge → discharge cycles.

use chrono::{DateTime, Utc};

use crate::learning::pair_diagnostics::PairDiagnostics;
use crate::learning::pairing::pair_sessions;
use crate::learning::robust::{median as plain_median, weighted_median};
use crate::learning::session::SessionRecord;

/// Accepted cycles needed before a measurement replaces the profile.
///
/// The fleet cycles about once a day: one charge block and one evening discharge. Three couples
/// are three days of evidence inside the 30-day session window, enough for a weighted median to
/// survive one bad run without delaying a real measurement for weeks. Fewer would let a single
/// mis-metered cycle set the pack's efficiency.
pub const MIN_PAIRS: usize = 3;
/// Beyond this idle time a charge and a discharge are not one cycle any more.
///
/// The fleet holds a full pack for hours, not days; a week is a generous ceiling for a hold and
/// keeps a long-abandoned charge from being booked as a loss.
pub const MAX_PAIR_GAP_DAYS: f64 = 7.0;
/// The discharge must return the pack to the charge's start within this fraction of its span...
pub const CYCLE_TOLERANCE_FRACTION: f64 = 0.15;
/// ...or this many points when the span is small and a fraction would be tighter than the BMS
/// reports in (whole percent).
pub const CYCLE_TOLERANCE_PCT: f64 = 2.0;
/// SoC drift the pack may show between the two legs before the pair is measuring a leak.
pub const HOLD_DRIFT_FRACTION: f64 = 0.10;
/// The same drift floor in points.
pub const HOLD_DRIFT_PCT: f64 = 1.0;
/// A home pack cannot round-trip below half its AC energy.
pub const MIN_ROUND_TRIP: f64 = 0.50;
/// Losses are losses: an AC round trip above 1 is a measurement error.
pub const MAX_ROUND_TRIP: f64 = 1.00;
/// Implied capacity envelope; a pair outside it measured something other than the pack.
pub const MIN_IMPLIED_CAPACITY_KWH: f64 = 1.0;
/// Upper end of that envelope.
pub const MAX_IMPLIED_CAPACITY_KWH: f64 = 200.0;
/// The capacity a pair assumes when the pack has no usable label, kWh (one AB2000).
///
/// Only the *correction term* of a cycle uses it: an exactly closed cycle does not touch it, and a
/// deep one is weighted down by `I / (I + |net|)`, so an unknown label cannot swing the estimate.
pub const FALLBACK_CAPACITY_KWH: f64 = 1.92;
/// Typed fallback until the pack has taught us better: 90 % per leg, so 81 % AC-to-AC.
///
/// Both numbers are the same convention the UI uses; 81 % is what a healthy LFP pack on this AC
/// path actually delivers, not the 95 % a datasheet quotes for the DC cells.
pub const DEFAULT_ROUND_TRIP: f64 = 0.81;
/// Largest median absolute deviation of accepted round trips (one vote per couple) that still
/// counts as agreement (absolute AC-to-AC). Beyond this the cycles measured different things.
pub const AGREEMENT_BAND: f64 = 0.02;
/// Effective cycles at which confidence saturates (twice the application minimum).
const CONFIDENCE_CYCLES: f64 = 2.0 * MIN_PAIRS as f64;
/// A measurement needs this much confidence before it may be published or retained.
///
/// At three effective couples this requires `MAD ≤ 0.008`, well inside the agreement band; a
/// near-boundary cluster of three couples is information, not an accurate measurement.
pub const MIN_CONFIDENCE: f64 = 0.30;

/// The pack's measured round trip, split evenly into the two legs.
///
/// `eta_charge = eta_discharge = sqrt(round_trip)` is the same convention the UI and the typed
/// configuration use, so a learned profile can be compared with a hand-entered one.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficiencyEstimate {
    /// Serial (or stable ledger id) of the pack.
    pub pack_id: String,
    /// AC-to-AC round trip in `(0, 1]`.
    pub round_trip: f64,
    /// Per-leg efficiency both legs share, `sqrt(round_trip)`.
    pub eta_charge: f64,
    /// The same number for the discharge leg.
    pub eta_discharge: f64,
    /// Accepted closed cycles this pass.
    pub pairs: usize,
    /// Charge/discharge couples the pairing refused.
    pub pairs_refused: usize,
    /// Sessions seen, candidates tried, refusal reasons and implied round trips.
    pub diagnostics: PairDiagnostics,
    /// Published confidence in `[0, 1]`, from effective sample size and agreement.
    pub confidence: f64,
    /// Whether *this* pass published a fresh measurement.
    pub applied: bool,
    /// Whether the previous confident profile was kept because this pass had too little data.
    pub kept_previous: bool,
    /// When the pass ran.
    pub measured_at: DateTime<Utc>,
}

impl EfficiencyEstimate {
    /// The typed profile: used until the pack has three closed cycles to show.
    pub fn default_profile(pack_id: &str, measured_at: DateTime<Utc>) -> Self {
        let split = DEFAULT_ROUND_TRIP.sqrt();
        Self {
            pack_id: pack_id.to_owned(),
            round_trip: DEFAULT_ROUND_TRIP,
            eta_charge: split,
            eta_discharge: split,
            pairs: 0,
            pairs_refused: 0,
            diagnostics: PairDiagnostics::default(),
            confidence: 0.0,
            applied: false,
            kept_previous: false,
            measured_at,
        }
    }

    /// Learn from the pack's sessions, keeping a confident previous profile on thin or
    /// contradictory data.
    ///
    /// Application needs at least [`MIN_PAIRS`] couples, at least that many *effective* couples
    /// (so one deep cycle cannot carry the estimate), a dispersion inside [`AGREEMENT_BAND`] and
    /// [`MIN_CONFIDENCE`] actual confidence. A blocked pass keeps a measured previous profile, or
    /// the typed default, and names the blocker in the diagnostics.
    pub fn from_sessions(
        pack_id: &str,
        sessions: &[SessionRecord],
        label_kwh: f64,
        now: DateTime<Utc>,
        previous: Option<&Self>,
    ) -> Self {
        let (pairs, mut diagnostics) = pair_sessions(sessions, pack_id, label_kwh);
        diagnostics.last_attempt_at = Some(now);
        diagnostics.last_learning_at = Some(now);
        diagnostics.sessions_in_window = sessions.len();
        diagnostics.sessions_complete = sessions.iter().filter(|session| session.complete).count();
        let samples: Vec<(f64, f64)> = pairs
            .iter()
            .map(|pair| (pair.round_trip, pair.weight))
            .collect();
        let evidence = evidence(&samples);
        let measured_confidence = evidence.as_ref().map_or(0.0, confidence);
        let blocker = evidence.as_ref().map_or(Some("no_measurement"), |value| {
            blocker(pairs.len(), value, measured_confidence)
        });
        let (effective_pairs, dispersion) = evidence.as_ref().map_or((0.0, 0.0), |value| {
            (value.effective_pairs, value.dispersion)
        });
        let confidence = if blocker.is_none() {
            measured_confidence
        } else {
            0.0
        };
        diagnostics.set_evidence(
            pairs.len(),
            effective_pairs,
            dispersion,
            measured_confidence,
            blocker,
        );
        let summary = diagnostics.summary();
        tracing::debug!(pack_id, %summary, "efficiency pairing");
        let mut estimate = Self::default_profile(pack_id, now);
        estimate.pairs = pairs.len();
        estimate.pairs_refused = diagnostics.refused;
        estimate.diagnostics = diagnostics.clone();
        if blocker.is_some() {
            return match previous.filter(|prior| prior.is_confident()) {
                Some(prior) => Self::kept(prior, pack_id, now, pairs.len(), diagnostics),
                None => estimate,
            };
        }
        let Some(evidence) = evidence else {
            return estimate;
        };
        let round_trip = evidence.median.clamp(MIN_ROUND_TRIP, MAX_ROUND_TRIP);
        let split = round_trip.sqrt();
        Self {
            round_trip,
            eta_charge: split,
            eta_discharge: split,
            pairs: pairs.len(),
            pairs_refused: diagnostics.refused,
            confidence,
            applied: true,
            kept_previous: false,
            diagnostics,
            ..Self::default_profile(pack_id, now)
        }
    }

    /// `x = eta_charge = eta_discharge`: the converter from AC energy to SoC points.
    ///
    /// Clamped to the plausible per-leg range, so a corrupt persisted profile cannot inflate a
    /// capacity by claiming a round trip above 1.
    pub fn split(&self) -> f64 {
        self.round_trip.clamp(MIN_ROUND_TRIP, MAX_ROUND_TRIP).sqrt()
    }

    /// Whether the value in hand is accurate enough to publish or carry forward.
    ///
    /// A stale stored profile can claim `applied`; its confidence still decides whether the next
    /// pass may keep it.
    pub const fn is_confident(&self) -> bool {
        self.confidence >= MIN_CONFIDENCE
    }

    /// A stable slug for logs and the UI.
    pub const fn slug(&self) -> &'static str {
        if self.applied {
            "efficiency_applied"
        } else if self.kept_previous {
            "efficiency_kept_previous"
        } else {
            "efficiency_default"
        }
    }

    /// The previous profile, re-stamped for this pass with the new evidence attached.
    fn kept(
        prior: &Self,
        pack_id: &str,
        now: DateTime<Utc>,
        pairs: usize,
        diagnostics: PairDiagnostics,
    ) -> Self {
        Self {
            pack_id: pack_id.to_owned(),
            round_trip: prior.round_trip,
            eta_charge: prior.eta_charge,
            eta_discharge: prior.eta_discharge,
            pairs,
            pairs_refused: diagnostics.refused,
            diagnostics,
            confidence: prior.confidence,
            applied: false,
            kept_previous: true,
            measured_at: now,
        }
    }
}

/// The accepted couples' weighted median, Kish effective size and unweighted spread.
struct Evidence {
    median: f64,
    effective_pairs: f64,
    dispersion: f64,
}

fn evidence(samples: &[(f64, f64)]) -> Option<Evidence> {
    let median = weighted_median(samples)?;
    let weights: Vec<f64> = samples
        .iter()
        .map(|&(value, weight)| {
            if value.is_finite() && weight.is_finite() && weight > 0.0 {
                weight
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = weights.iter().sum();
    let squares: f64 = weights.iter().map(|weight| weight * weight).sum();
    if total <= 0.0 || squares <= 0.0 || !total.is_finite() || !squares.is_finite() {
        return None;
    }
    let deviations: Vec<f64> = samples
        .iter()
        .map(|(value, _)| (value - median).abs())
        .collect();
    Some(Evidence {
        median,
        effective_pairs: total * total / squares,
        dispersion: plain_median(&deviations).unwrap_or(0.0),
    })
}

/// Why this pass may not publish a measurement.
fn blocker(pairs: usize, evidence: &Evidence, confidence: f64) -> Option<&'static str> {
    if pairs < MIN_PAIRS {
        Some("too_few_pairs")
    } else if !evidence.median.is_finite() || !evidence.dispersion.is_finite() {
        Some("no_measurement")
    } else if evidence.effective_pairs < MIN_PAIRS as f64 {
        Some("too_few_effective_pairs")
    } else if evidence.dispersion > AGREEMENT_BAND {
        Some("disagreement")
    } else {
        (confidence < MIN_CONFIDENCE).then_some("low_confidence")
    }
}

/// `confidence = min(n_eff / (2 · MIN_PAIRS), 1) · clamp(1 - MAD / AGREEMENT_BAND, 0, 1)`.
/// The median is weighted, the MAD is not: one heavy cycle may move the value but cannot hide
/// disagreement among the other couples.
fn confidence(evidence: &Evidence) -> f64 {
    let information = (evidence.effective_pairs / CONFIDENCE_CYCLES).min(1.0);
    let agreement = (1.0 - evidence.dispersion / AGREEMENT_BAND).clamp(0.0, 1.0);
    information * agreement
}
