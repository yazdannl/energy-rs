//! Largest contiguous cheap run, shared by complete and partial maintenance charges.

use crate::protection::full_charge::{ChargeCandidate, ChargeSlot, DuePack};

/// Reserved slots by eligible index, cost EUR, delivered AC kWh, and whether the need was met.
pub(crate) type FilledWindow = (Vec<(usize, ChargeSlot)>, f64, f64, bool);

const ENERGY_EPS_KWH: f64 = 1e-6;
/// Price difference that counts as a revised publication rather than a rounding artefact.
const PRICE_EPS_EUR: f64 = 1e-6;

/// Only ceilings from this night and this price publication may anchor the band.
pub(crate) fn anchored_ceiling(
    previous: &[crate::protection::full_charge::FullChargeAssignment],
    deadline: i64,
    candidates: &[ChargeCandidate],
) -> f64 {
    previous
        .iter()
        .filter(|entry| entry.band_deadline == deadline)
        .filter(|entry| {
            !entry
                .slots
                .iter()
                .any(|slot| price_changed(candidates, slot))
        })
        .map(|entry| entry.band_ceil_eur_per_kwh)
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Whether a booked slot's price was revised since it was planned.
fn price_changed(candidates: &[ChargeCandidate], slot: &ChargeSlot) -> bool {
    candidates.iter().any(|candidate| {
        candidate.slot_start == slot.slot_start
            && (candidate.price_eur_per_kwh - slot.price_eur_per_kwh).abs() > PRICE_EPS_EUR
    })
}

/// Fill the temporally contiguous run with the most deliverable energy.
///
/// Expensive gaps and slots other packs have exhausted end a run. Ties go to the earliest start,
/// so a cheap island never gets joined to another through quarters that are not eligible.
pub(crate) fn fill(
    pack: &DuePack,
    eligible: &[ChargeCandidate],
    remaining: &[f64],
    needed: f64,
) -> FilledWindow {
    let mut best: FilledWindow = (Vec::new(), 0.0, 0.0, false);
    for start in 0..eligible.len() {
        if remaining[start] <= 0.0 {
            continue;
        }
        let run = fill_run(pack, eligible, remaining, start, needed);
        if better(&run, &best) {
            best = run;
        }
    }
    best
}

/// A complete run beats a partial one; otherwise prefer more energy, then the earlier run.
fn better(candidate: &FilledWindow, best: &FilledWindow) -> bool {
    let (_, _, energy, complete) = *candidate;
    let (_, _, best_energy, best_complete) = *best;
    if complete != best_complete {
        return complete;
    }
    if (energy - best_energy).abs() > ENERGY_EPS_KWH {
        return energy > best_energy;
    }
    first_start(candidate) < first_start(best)
}

fn first_start(run: &FilledWindow) -> i64 {
    run.0.first().map_or(i64::MAX, |(_, slot)| slot.t_start)
}

/// Walk one run from `start`, stopping at a price gap or an exhausted slot.
fn fill_run(
    pack: &DuePack,
    eligible: &[ChargeCandidate],
    remaining: &[f64],
    start: usize,
    needed: f64,
) -> FilledWindow {
    let mut reserved = remaining.to_vec();
    let mut unfilled = needed;
    let mut slots = Vec::new();
    let mut cost = 0.0;
    let mut delivered = 0.0;
    let mut complete = false;
    let mut previous_end: Option<i64> = None;
    for index in start..eligible.len() {
        let candidate = eligible[index];
        if previous_end.is_some_and(|end| candidate.slot_start != end) {
            break;
        }
        let seconds = candidate.t_end - candidate.t_start;
        if seconds <= 0 {
            continue;
        }
        let watts = reserved[index]
            .min(pack.max_charge_w)
            .min(unfilled * 3_600_000.0 / seconds as f64)
            .max(0.0);
        if watts <= 0.0 {
            break;
        }
        let energy = watts * seconds as f64 / 3_600_000.0;
        cost += energy * candidate.price_eur_per_kwh;
        delivered += energy;
        unfilled -= energy;
        slots.push((
            index,
            ChargeSlot {
                slot_start: candidate.slot_start,
                t_start: candidate.t_start,
                t_end: candidate.t_end,
                duration_s: seconds,
                price_eur_per_kwh: candidate.price_eur_per_kwh,
                charge_w: watts,
            },
        ));
        reserved[index] -= watts;
        previous_end = Some(candidate.t_end);
        if unfilled <= ENERGY_EPS_KWH {
            complete = true;
            break;
        }
    }
    (slots, cost, delivered, complete)
}

/// Capacity of a slot over its whole published quarter-hour, ignoring elapsed seconds.
///
/// `missing_kwh` is measured against this figure: a replan later in the same slot must not turn
/// already-elapsed time into new shortfall for a pack that was never booked there.
pub(crate) fn nominal_kwh(slot: &ChargeCandidate, watts: f64, pack_max_w: f64) -> f64 {
    let seconds = (slot.t_end - slot.slot_start).max(0) as f64;
    watts.min(pack_max_w).max(0.0) * seconds / 3_600_000.0
}

/// Full-quarter-hour capacity left in the cheap band after earlier packs' reservations.
pub(crate) fn nominal_available(
    pack: &DuePack,
    eligible: &[ChargeCandidate],
    remaining: &[f64],
) -> f64 {
    eligible
        .iter()
        .zip(remaining.iter())
        .map(|(slot, watts)| nominal_kwh(slot, *watts, pack.max_charge_w))
        .sum()
}

/// Why a partial fill could not cover the whole requirement.
pub(crate) fn shortfall_reason(band_kwh: f64, needed: f64) -> &'static str {
    if band_kwh + 1e-6 < needed {
        "not_enough_eligible_slots"
    } else {
        "insufficient_capacity"
    }
}
