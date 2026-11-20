//! The seven-day, price-aware maintenance rule.
//!
//! A pack is due when its last *completed* balancing dwell is older than
//! [`FullChargeSettings::interval_days`] (or a pack with no history at all). "Cheap" is concrete:
//! lower half of the published horizon's prices *and* at or below its mean, by all-in EUR/kWh.
//! From that half we take the cheapest quarter-hours that cover the AC energy missing to reach
//! 100 %, limited by the grid
//! import budget and the pack's own charge ceiling. With 120 slots the eligible half is 60 slots,
//! i.e. 15 h at one pack's 800 W: enough for four AB2000 packs from 5 % (4 × 1.92 kWh AC ≈ 7.7 kWh;
//! the 60 cheapest slots hold roughly 12 kWh at 800 W). If the eligible slots cannot cover a pack,
//! it gets no assignment: the rule is "the first cheap chance", not "charge at any price".

use crate::protection::full_charge::assignment::FullChargeAssignment;
use crate::protection::full_charge::candidate::ChargeCandidate;
use crate::protection::full_charge::charge_slot::ChargeSlot;
use crate::protection::full_charge::due_pack::DuePack;
use crate::protection::full_charge::settings::FullChargeSettings;

/// Tolerance for treating a pack's missing energy as zero.
const ENERGY_EPS_KWH: f64 = 1e-6;

/// Plan maintenance charges for the packs that are due at `now`.
///
/// `budget_w` is the caller's grid-import budget after the configured margin; candidates already
/// carry the house's own import. Both are ceilings, never targets: the planner never asks for more
/// than either allows, and the dispatch ladder still applies every hardware and cell bound.
pub fn plan(
    now: i64,
    settings: &FullChargeSettings,
    candidates: &[ChargeCandidate],
    packs: &[DuePack],
    budget_w: f64,
) -> Vec<FullChargeAssignment> {
    if !settings.enabled || candidates.is_empty() || !budget_w.is_finite() || budget_w <= 0.0 {
        return Vec::new();
    }
    let Some(threshold) = price_threshold(candidates) else {
        return Vec::new();
    };
    let mut eligible: Vec<ChargeCandidate> = candidates
        .iter()
        .copied()
        .filter(|slot| slot.price_eur_per_kwh <= threshold && slot.available_w > 0.0)
        .collect();
    eligible.sort_by(|a, b| {
        a.price_eur_per_kwh
            .total_cmp(&b.price_eur_per_kwh)
            .then(a.t_start.cmp(&b.t_start))
    });
    if eligible.is_empty() {
        return Vec::new();
    }
    let mut remaining: Vec<f64> = eligible
        .iter()
        .map(|slot| slot.available_w.min(budget_w).max(0.0))
        .collect();
    let mut ordered: Vec<&DuePack> = packs
        .iter()
        .filter(|pack| pack.due(now, settings.interval_days()))
        .collect();
    ordered.sort_by_key(|pack| (pack.last_full_at.unwrap_or(i64::MIN), pack.pack_id.clone()));

    let mut assignments = Vec::new();
    for pack in ordered {
        if let Some(assignment) = assign(pack, &eligible, &mut remaining, settings, now) {
            assignments.push(assignment);
        }
    }
    assignments
}

/// A cheap slot is in the lower half of the horizon *and* below its mean price.
///
/// The mean bound matters when a short dip sits next to a long expensive stretch: a plain median
/// would classify the expensive stretch as cheap simply because it occupies more than half the
/// horizon, and the planner would charge at the very price it was meant to avoid.
fn price_threshold(candidates: &[ChargeCandidate]) -> Option<f64> {
    let mut prices: Vec<f64> = candidates
        .iter()
        .map(|slot| slot.price_eur_per_kwh)
        .filter(|price| price.is_finite())
        .collect();
    if prices.is_empty() {
        return None;
    }
    prices.sort_by(f64::total_cmp);
    let median = prices.get((prices.len() - 1) / 2).copied()?;
    let mean = prices.iter().sum::<f64>() / prices.len() as f64;
    Some(median.min(mean))
}

/// Fill one pack from the cheapest slots that still have import headroom.
fn assign(
    pack: &DuePack,
    eligible: &[ChargeCandidate],
    remaining: &mut [f64],
    settings: &FullChargeSettings,
    now: i64,
) -> Option<FullChargeAssignment> {
    let reason = if pack.forced {
        "manual_request"
    } else {
        "interval_due"
    };
    let mut reserved = remaining.to_vec();
    let mut needed = pack.ac_energy_needed_kwh();
    let mut slots = Vec::new();
    let mut cost = 0.0;
    let mut charge_end = now;
    if needed <= ENERGY_EPS_KWH {
        charge_end = now;
    } else if !pack.max_charge_w.is_finite() || pack.max_charge_w <= 0.0 {
        return None;
    } else {
        for (index, candidate) in eligible.iter().enumerate() {
            if needed <= ENERGY_EPS_KWH {
                break;
            }
            let hours = (candidate.t_end - candidate.t_start) as f64 / 3600.0;
            if hours <= 0.0 {
                continue;
            }
            let watts = reserved[index]
                .min(pack.max_charge_w)
                .min(needed * 1000.0 / hours)
                .max(0.0);
            if watts <= 0.0 {
                continue;
            }
            let energy = watts * hours / 1000.0;
            needed -= energy;
            cost += watts * hours / 1000.0 * candidate.price_eur_per_kwh;
            charge_end = charge_end.max(candidate.t_end);
            slots.push(ChargeSlot {
                t_start: candidate.t_start,
                t_end: candidate.t_end,
                price_eur_per_kwh: candidate.price_eur_per_kwh,
                charge_w: watts,
            });
            reserved[index] -= watts;
        }
    }
    if needed > ENERGY_EPS_KWH {
        // No partial maintenance charge: the caller's capacities were never touched.
        return None;
    }
    remaining.copy_from_slice(&reserved);
    slots.sort_by_key(|slot| slot.t_start);
    Some(FullChargeAssignment {
        pack_id: pack.pack_id.clone(),
        reason,
        target_soc_pct: 100.0,
        slots,
        charge_end,
        dwell_until: charge_end + i64::from(settings.dwell_minutes()) * 60,
        expected_cost_eur: cost,
    })
}
