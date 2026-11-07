//! Round-trip efficiency from closed charge → discharge cycles.

use chrono::{DateTime, Utc};

use crate::control::Direction;
use crate::learning::cycle::CyclePair;
use crate::learning::robust::weighted_median;
use crate::learning::session::SessionRecord;

/// Accepted cycles needed before a measurement replaces the profile.
pub const MIN_PAIRS: usize = 3;
/// Beyond this idle time a charge and a discharge are not one cycle any more.
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
    /// Adjacent charge/discharge couples the pairing refused.
    pub pairs_refused: usize,
    /// Confidence in `[0, 1]`, from the accepted cycle count.
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
            confidence: 0.0,
            applied: false,
            kept_previous: false,
            measured_at,
        }
    }

    /// Learn from the pack's sessions, keeping a confident previous profile on thin data.
    ///
    /// Fewer than [`MIN_PAIRS`] accepted cycles never wipe a profile that was measured: the pass
    /// reports `kept_previous` and carries the previous numbers forward. With no previous measurement
    /// the typed [`DEFAULT_ROUND_TRIP`] stands.
    pub fn from_sessions(
        pack_id: &str,
        sessions: &[SessionRecord],
        label_kwh: f64,
        now: DateTime<Utc>,
        previous: Option<&Self>,
    ) -> Self {
        let (pairs, refused) = pair_cycles(sessions, pack_id, label_kwh);
        let mut estimate = Self::default_profile(pack_id, now);
        estimate.pairs = pairs.len();
        estimate.pairs_refused = refused;
        if pairs.len() < MIN_PAIRS {
            return match previous.filter(|prior| prior.is_confident()) {
                Some(prior) => Self {
                    pairs: pairs.len(),
                    pairs_refused: refused,
                    ..Self::kept(prior, pack_id, now)
                },
                None => estimate,
            };
        }
        let samples: Vec<(f64, f64)> = pairs
            .iter()
            .map(|pair| (pair.round_trip, pair.weight))
            .collect();
        let Some(round_trip) = weighted_median(&samples) else {
            return estimate;
        };
        let round_trip = round_trip.clamp(MIN_ROUND_TRIP, MAX_ROUND_TRIP);
        let split = round_trip.sqrt();
        Self {
            round_trip,
            eta_charge: split,
            eta_discharge: split,
            pairs: pairs.len(),
            pairs_refused: refused,
            confidence: confidence(pairs.len()),
            applied: true,
            kept_previous: false,
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

    /// Whether the value in hand is a measurement (fresh or carried forward).
    pub const fn is_confident(&self) -> bool {
        self.applied || self.kept_previous
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

    /// The previous profile, re-stamped for this pass.
    fn kept(prior: &Self, pack_id: &str, now: DateTime<Utc>) -> Self {
        Self {
            pack_id: pack_id.to_owned(),
            round_trip: prior.round_trip,
            eta_charge: prior.eta_charge,
            eta_discharge: prior.eta_discharge,
            pairs: 0,
            pairs_refused: 0,
            confidence: prior.confidence,
            applied: false,
            kept_previous: true,
            measured_at: now,
        }
    }
}

/// Confidence rises with the cycle count and reaches 1 at twice [`MIN_PAIRS`].
fn confidence(pairs: usize) -> f64 {
    (pairs as f64 / (2.0 * MIN_PAIRS as f64)).min(1.0)
}

/// Pair every charge with the discharge that immediately follows it.
///
/// Returns the accepted pairs and how many adjacent couples were refused. Only *consecutive*
/// sessions are candidates: a charge and an unrelated discharge days later are two half cycles, and
/// comparing them would book stored energy as a loss.
fn pair_cycles(
    sessions: &[SessionRecord],
    pack_id: &str,
    label_kwh: f64,
) -> (Vec<CyclePair>, usize) {
    let mut ordered: Vec<&SessionRecord> = sessions
        .iter()
        .filter(|session| session.pack_id == pack_id && session.refusal().is_none())
        .collect();
    ordered.sort_by_key(|session| session.started_at);
    let mut pairs = Vec::new();
    let mut refused = 0_usize;
    for couple in ordered.windows(2) {
        match pair_one(couple[0], couple[1], label_kwh) {
            Some(pair) => pairs.push(pair),
            None => refused += 1,
        }
    }
    (pairs, refused)
}

/// One charge → discharge couple, or `None` when it is not a complete cycle.
fn pair_one(
    charge: &SessionRecord,
    discharge: &SessionRecord,
    label_kwh: f64,
) -> Option<CyclePair> {
    if charge.direction != Direction::Charge || discharge.direction != Direction::Discharge {
        return None;
    }
    let charge_span = charge.soc_span_pct();
    let discharge_span = discharge.soc_span_pct();
    if charge_span <= 0.0 || discharge_span <= 0.0 {
        return None;
    }
    let gap_s = (discharge.started_at - charge.ended_at).num_seconds();
    if gap_s < 0 || gap_s as f64 > MAX_PAIR_GAP_DAYS * 86_400.0 {
        return None;
    }
    let drift = (discharge.soc_start_pct - charge.soc_end_pct).abs();
    if drift > HOLD_DRIFT_PCT.max(HOLD_DRIFT_FRACTION * charge_span) {
        return None;
    }
    let tolerance = CYCLE_TOLERANCE_PCT.max(CYCLE_TOLERANCE_FRACTION * charge_span);
    let deeper = charge.soc_start_pct - discharge.soc_end_pct;
    if deeper < -tolerance {
        return None;
    }
    let assumed = if label_kwh.is_finite() && label_kwh > 0.0 {
        label_kwh
    } else {
        FALLBACK_CAPACITY_KWH
    };
    let charge_kwh = charge.energy_ac_kwh;
    let discharge_kwh = discharge.energy_ac_kwh;
    if charge_kwh <= 0.0 || discharge_kwh <= 0.0 {
        return None;
    }
    let net_kwh = (discharge.soc_end_pct - charge.soc_start_pct) / 100.0 * assumed;
    let root = (net_kwh * net_kwh + 4.0 * charge_kwh * discharge_kwh).sqrt();
    let split_eta = (net_kwh + root) / (2.0 * charge_kwh);
    let round_trip = split_eta * split_eta;
    let weight = charge_span.min(discharge_span) * (charge_kwh / (charge_kwh + net_kwh.abs()));
    let pair = CyclePair {
        charge_at: charge.started_at,
        discharge_at: discharge.started_at,
        charge_kwh,
        discharge_kwh,
        charge_span_pct: charge_span,
        discharge_span_pct: discharge_span,
        net_kwh,
        split_eta,
        round_trip,
        weight,
    };
    let implied = pair.implied_capacity_kwh()?;
    let plausible_trip =
        round_trip.is_finite() && (MIN_ROUND_TRIP..=MAX_ROUND_TRIP).contains(&round_trip);
    let plausible_capacity =
        (MIN_IMPLIED_CAPACITY_KWH..=MAX_IMPLIED_CAPACITY_KWH).contains(&implied);
    (plausible_trip && plausible_capacity).then_some(pair)
}
