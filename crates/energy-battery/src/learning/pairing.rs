//! Pair a pack's charge and discharge blocks into closed cycles.
//!
//! Real packs charge in several interrupted segments, hold at full SoC and discharge once. Treating
//! each session as a leg pairs charge with charge and drops the energy of every earlier segment; a
//! cycle is therefore a *block* of consecutive same-direction sessions. Blocks alternate by
//! construction, so each discharge is paired with the immediately preceding charge block and no
//! intervening direction change is skipped.

use crate::control::Direction;
use crate::learning::blocks::{Block, build_blocks};
use crate::learning::cycle::CyclePair;
use crate::learning::efficiency::{
    CYCLE_TOLERANCE_FRACTION, CYCLE_TOLERANCE_PCT, FALLBACK_CAPACITY_KWH, HOLD_DRIFT_FRACTION,
    HOLD_DRIFT_PCT, MAX_IMPLIED_CAPACITY_KWH, MAX_PAIR_GAP_DAYS, MAX_ROUND_TRIP,
    MIN_IMPLIED_CAPACITY_KWH, MIN_ROUND_TRIP,
};
use crate::learning::pair_attempt::PairAttempt;
use crate::learning::pair_diagnostics::PairDiagnostics;
use crate::learning::pair_refusal::PairRefusal;
use crate::learning::session::SessionRecord;

/// Pair this pack's sessions, returning accepted cycles and the full refusal distribution.
pub(crate) fn pair_sessions(
    sessions: &[SessionRecord],
    pack_id: &str,
    label_kwh: f64,
) -> (Vec<CyclePair>, PairDiagnostics) {
    let mut own: Vec<&SessionRecord> = sessions.iter().filter(|s| s.pack_id == pack_id).collect();
    let sessions_valid = own
        .iter()
        .filter(|session| session.refusal().is_none())
        .count();
    own.sort_by_key(|session| session.started_at);
    let mut splits = Vec::new();
    let grouped = build_blocks(&own, &mut splits);
    let mut diagnostics = PairDiagnostics {
        sessions_seen: own.len(),
        sessions_valid,
        ..PairDiagnostics::default()
    };
    let mut markers = record_splits(&mut diagnostics, splits);
    let mut pairs = Vec::new();
    for (index, discharge) in grouped.iter().enumerate() {
        if discharge.direction != Direction::Discharge {
            continue;
        }
        if let Some(reason) = discharge.refusal_slug() {
            let refusal = PairRefusal::UnusableLeg {
                at: discharge.started_at(),
                reason,
            };
            refuse(&mut diagnostics, discharge, &refusal);
            continue;
        }
        let prior = index.checked_sub(1).map(|position| &grouped[position]);
        let charge = match prior {
            Some(block) if block.direction == Direction::Charge && block.usable => block,
            Some(block) if block.direction == Direction::Charge => {
                let reason = PairRefusal::UnusableLeg {
                    at: discharge.started_at(),
                    reason: block.refusal_slug().unwrap_or("unusable_leg"),
                };
                refuse(&mut diagnostics, discharge, &reason);
                continue;
            }
            _ => {
                let refusal = PairRefusal::NoChargeBefore {
                    at: discharge.started_at(),
                };
                refuse(&mut diagnostics, discharge, &refusal);
                continue;
            }
        };
        let (pair, attempt) = attempt(charge, discharge, label_kwh);
        if let Some(pair) = pair {
            pairs.push(pair);
        }
        diagnostics.record(attempt);
    }
    markers.append(&mut diagnostics.attempts);
    diagnostics.set_attempts(markers);
    (pairs, diagnostics)
}

/// Count each split and keep a marker for the bounded attempt history.
fn record_splits(
    diagnostics: &mut PairDiagnostics,
    splits: Vec<(&'static str, chrono::DateTime<chrono::Utc>)>,
) -> Vec<PairAttempt> {
    let mut markers = Vec::new();
    for (reason, at) in splits {
        diagnostics.record_split(reason);
        markers.push(split_marker(reason, at));
    }
    markers
}

/// Record one refusal with a plain unpaired attempt.
fn refuse(diagnostics: &mut PairDiagnostics, discharge: &Block<'_>, reason: &PairRefusal) {
    diagnostics.record(refused(unpaired(discharge), reason, None));
}

/// A split marker is a boundary event, not a discharge candidate.
fn split_marker(reason: &'static str, at: chrono::DateTime<chrono::Utc>) -> PairAttempt {
    PairAttempt {
        at,
        reason: reason.to_owned(),
        accepted: false,
        round_trip: None,
        charge_kwh: 0.0,
        discharge_kwh: 0.0,
        soc_from_pct: 0.0,
        soc_to_pct: 0.0,
        charge_segments: 0,
    }
}

/// A discharge with no preceding charge has no energy in and no cycle span.
fn unpaired(discharge: &Block<'_>) -> PairAttempt {
    PairAttempt {
        at: discharge.started_at(),
        reason: "accepted".to_owned(),
        accepted: true,
        round_trip: None,
        charge_kwh: 0.0,
        discharge_kwh: discharge.energy_ac_kwh,
        soc_from_pct: discharge.soc_start_pct,
        soc_to_pct: discharge.soc_end_pct,
        charge_segments: 0,
    }
}

/// The plain facts of one candidate, before any predicate has judged it.
fn base(charge: &Block<'_>, discharge: &Block<'_>) -> PairAttempt {
    let naive = discharge.energy_ac_kwh / charge.energy_ac_kwh;
    PairAttempt {
        at: discharge.started_at(),
        reason: "accepted".to_owned(),
        accepted: true,
        round_trip: naive.is_finite().then_some(naive),
        charge_kwh: charge.energy_ac_kwh,
        discharge_kwh: discharge.energy_ac_kwh,
        soc_from_pct: charge.soc_start_pct,
        soc_to_pct: discharge.soc_end_pct,
        charge_segments: charge.sessions.len(),
    }
}

fn refused(mut attempt: PairAttempt, reason: &PairRefusal, round_trip: Option<f64>) -> PairAttempt {
    reason.slug().clone_into(&mut attempt.reason);
    attempt.accepted = false;
    if let Some(value) = round_trip {
        attempt.round_trip = Some(value);
    }
    attempt
}

/// The structural predicates: the two legs must be one held, closed, non-empty cycle.
fn structural(charge: &Block<'_>, discharge: &Block<'_>) -> Option<PairRefusal> {
    let at = discharge.started_at();
    let gap_s = (at - charge.ended_at).num_seconds();
    if gap_s < 0 {
        return Some(PairRefusal::OverlappingLegs { at });
    }
    if gap_s as f64 > MAX_PAIR_GAP_DAYS * 86_400.0 {
        return Some(PairRefusal::GapTooLong {
            at,
            days: gap_s as f64 / 86_400.0,
        });
    }
    let drift = (discharge.soc_start_pct - charge.soc_end_pct).abs();
    if drift > HOLD_DRIFT_PCT.max(HOLD_DRIFT_FRACTION * charge.span_pct()) {
        return Some(PairRefusal::HoldDrift {
            at,
            drift_pct: drift,
        });
    }
    let tolerance = CYCLE_TOLERANCE_PCT.max(CYCLE_TOLERANCE_FRACTION * charge.span_pct());
    let end_pct = discharge.soc_end_pct;
    if charge.soc_start_pct - end_pct < -tolerance {
        return Some(PairRefusal::NotClosed { at, end_pct });
    }
    if charge.energy_ac_kwh <= 0.0 || discharge.energy_ac_kwh <= 0.0 {
        return Some(PairRefusal::NoEnergy { at });
    }
    None
}

/// Invert the two legs' energy balance, including the span's net stored energy.
fn measure(charge: &Block<'_>, discharge: &Block<'_>, label_kwh: f64) -> Option<CyclePair> {
    let charge_span = charge.span_pct();
    let discharge_span = discharge.span_pct();
    if charge_span <= 0.0 || discharge_span <= 0.0 {
        return None;
    }
    let charge_kwh = charge.energy_ac_kwh;
    let discharge_kwh = discharge.energy_ac_kwh;
    let assumed = if label_kwh.is_finite() && label_kwh > 0.0 {
        label_kwh
    } else {
        FALLBACK_CAPACITY_KWH
    };
    let net_kwh = (discharge.soc_end_pct - charge.soc_start_pct) / 100.0 * assumed;
    let root = (net_kwh * net_kwh + 4.0 * charge_kwh * discharge_kwh).sqrt();
    let split_eta = (net_kwh + root) / (2.0 * charge_kwh);
    let round_trip = split_eta * split_eta;
    let weight = charge_span.min(discharge_span) * (charge_kwh / (charge_kwh + net_kwh.abs()));
    Some(CyclePair {
        charge_at: charge.started_at(),
        discharge_at: discharge.started_at(),
        charge_kwh,
        discharge_kwh,
        charge_span_pct: charge_span,
        discharge_span_pct: discharge_span,
        net_kwh,
        split_eta,
        round_trip,
        weight,
    })
}

/// Measure one candidate, or say exactly which predicate refused it.
fn attempt(
    charge: &Block<'_>,
    discharge: &Block<'_>,
    label_kwh: f64,
) -> (Option<CyclePair>, PairAttempt) {
    let record = base(charge, discharge);
    if let Some(reason) = structural(charge, discharge) {
        return (None, refused(record, &reason, None));
    }
    let Some(pair) = measure(charge, discharge, label_kwh) else {
        let reason = PairRefusal::NotFinite {
            at: discharge.started_at(),
        };
        return (None, refused(record, &reason, None));
    };
    if !pair.round_trip.is_finite() || !(MIN_ROUND_TRIP..=MAX_ROUND_TRIP).contains(&pair.round_trip)
    {
        let reason = PairRefusal::ImplausibleRoundTrip {
            at: discharge.started_at(),
            round_trip: pair.round_trip,
        };
        return (None, refused(record, &reason, Some(pair.round_trip)));
    }
    let Some(implied) = pair.implied_capacity_kwh() else {
        let reason = PairRefusal::NotFinite {
            at: discharge.started_at(),
        };
        return (None, refused(record, &reason, Some(pair.round_trip)));
    };
    if !(MIN_IMPLIED_CAPACITY_KWH..=MAX_IMPLIED_CAPACITY_KWH).contains(&implied) {
        let reason = PairRefusal::ImplausibleCapacity {
            at: discharge.started_at(),
            capacity_kwh: implied,
        };
        return (None, refused(record, &reason, Some(pair.round_trip)));
    }
    let round_trip = pair.round_trip;
    (Some(pair), accepted(record, round_trip))
}

fn accepted(mut attempt: PairAttempt, round_trip: f64) -> PairAttempt {
    attempt.accepted = true;
    "accepted".clone_into(&mut attempt.reason);
    attempt.round_trip = Some(round_trip);
    attempt
}
