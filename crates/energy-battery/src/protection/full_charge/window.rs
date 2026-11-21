//! Earliest-first slot filling, shared by complete and partial maintenance charges.

use crate::protection::full_charge::{ChargeCandidate, ChargeSlot, DuePack};

/// Reserved slots by eligible index, cost EUR, delivered AC kWh, and whether the need was met.
pub(crate) type FilledWindow = (Vec<(usize, ChargeSlot)>, f64, f64, bool);

/// Fill one run of eligible slots from `start`, skipping slots others have exhausted.
///
/// Watts never exceed the pack's ceiling or the headroom left in a slot. The returned energy is
/// what the plan can really deliver in the remaining seconds, so a partial fill is never counted
/// as a complete charge.
pub(crate) fn fill(
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
    for index in start..eligible.len() {
        let candidate = eligible[index];
        let seconds = candidate.t_end - candidate.t_start;
        if seconds <= 0 {
            continue;
        }
        let watts = reserved[index]
            .min(pack.max_charge_w)
            .min(unfilled * 3_600_000.0 / seconds as f64)
            .max(0.0);
        if watts <= 0.0 {
            continue;
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
        if unfilled <= 1e-6 {
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
