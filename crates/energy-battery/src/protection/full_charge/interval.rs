//! The seven-day, price-aware maintenance rule.
//!
//! A pack is due when its last *completed* balancing dwell is older than
//! [`FullChargeSettings::interval_days`] (or a pack with no history at all). "Cheap" is concrete:
//! lower half of the published horizon's prices *and* at or below its mean, by all-in EUR/kWh.
//!
//! The rule is "the first cheap charging chance tonight", not "the globally cheapest slots".
//! The completion deadline is exact: the next **07:00 Europe/Amsterdam**, the end of the overnight
//! cheap window before the morning peak. Only slots ending at or before that deadline are
//! considered, so tomorrow's lower price can never displace tonight's balancing charge. The
//! cheap band is computed from that same overnight window (the lower of its median and mean
//! all-in price): with a full multi-day publication tomorrow's slots are outside the window, and
//! when only today's prices are published the band simply uses the published part of tonight.
//! Within the band, slots are tried by earliest start (price breaks ties), and the first window
//! that can deliver the whole charge before the deadline wins. When no window can complete a pack,
//! the planner still books every watt the band has left: a partial charge, reported as such with
//! its missing kWh and cause. A pack with no bookable energy stays unassigned with its cause, and
//! is never reported as complete. Missing kWh is measured against each plan slot's published
//! quarter-hour, not the seconds left in the current slot: a replan inside a slot cannot invent a
//! new shortfall merely by waiting. The band ceiling is carried from the previous plan, so a
//! shrinking remaining-window median cannot demote an already-booked slot or open a hole in front
//! of it; the earliest eligible run always wins over later, marginally cheaper slots.

use chrono::{DateTime, LocalResult, TimeZone, Timelike};
use chrono_tz::Europe::Amsterdam;

use crate::protection::full_charge::unassigned_pack::{ceiling_refusal, shortfall, unassigned_all};
use crate::protection::full_charge::window::{fill, nominal_available, shortfall_reason};
use crate::protection::full_charge::{
    ChargeCandidate, ChargeSlot, DuePack, FullChargeAssignment, FullChargeSettings, IntervalPlan,
    TOLERANCE_PCT,
};

/// Tolerance for treating a pack's missing energy as zero.
const ENERGY_EPS_KWH: f64 = 1e-6;
/// The overnight window ends at 07:00 local, before the morning peak.
const DEADLINE_LOCAL_HOUR: u32 = 7;

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
) -> IntervalPlan {
    plan_after(now, settings, candidates, packs, budget_w, &[])
}

/// The same plan, with the previous plan's band ceiling as a floor.
///
/// Once a slot was cheap enough to be booked, a replan that only moves `now` forward may not
/// demote it: the ceiling is carried forward so the charge keeps its original slots and the band
/// never opens a hole ahead of it. Cheaper later slots may still be added, but never displace the
/// earliest run.
pub fn plan_after(
    now: i64,
    settings: &FullChargeSettings,
    candidates: &[ChargeCandidate],
    packs: &[DuePack],
    budget_w: f64,
    previous: &[FullChargeAssignment],
) -> IntervalPlan {
    let deadline = deadline_after(now);
    let due = due_packs(now, settings, packs);
    if !settings.enabled {
        return unassigned_all(due, deadline, 1, "disabled");
    }
    if due.is_empty() {
        return IntervalPlan::default();
    }
    if !budget_w.is_finite() || budget_w <= 0.0 {
        return unassigned_all(due, deadline, 1, "no_headroom");
    }
    let band_floor = previous
        .iter()
        .map(|entry| entry.band_ceil_eur_per_kwh)
        .fold(f64::NEG_INFINITY, f64::max);
    let Some((eligible, mut remaining, priced_without_headroom, band_ceil)) =
        cheap_slots(now, deadline, candidates, budget_w, band_floor)
    else {
        return unassigned_all(due, deadline, 0, "horizon_too_short");
    };
    let mut answer = IntervalPlan::default();
    for pack in due {
        if pack.max_soc_pct < 100.0 - TOLERANCE_PCT {
            answer.unassigned.push(ceiling_refusal(pack, deadline));
            continue;
        }
        if let Some(assignment) = assign(pack, &eligible, &mut remaining, settings, now, band_ceil)
        {
            answer.assignments.push(assignment);
        } else {
            answer.unassigned.push(shortfall(
                pack,
                &eligible,
                &remaining,
                priced_without_headroom,
                deadline,
            ));
        }
    }
    answer
}

/// Due packs, oldest first so the pack that waited longest gets the first cheap window.
fn due_packs<'a>(
    now: i64,
    settings: &FullChargeSettings,
    packs: &'a [DuePack],
) -> Vec<&'a DuePack> {
    let mut due: Vec<&DuePack> = packs
        .iter()
        .filter(|pack| pack.due(now, settings.interval_days()))
        .collect();
    due.sort_by_key(|pack| (pack.last_full_at.unwrap_or(i64::MIN), pack.pack_id.clone()));
    due
}

/// The eligible overnight slots and the grid headroom still free in each one.
fn cheap_slots(
    now: i64,
    deadline: i64,
    candidates: &[ChargeCandidate],
    budget_w: f64,
    band_floor: f64,
) -> Option<(Vec<ChargeCandidate>, Vec<f64>, bool, f64)> {
    let in_window: Vec<ChargeCandidate> = candidates
        .iter()
        .copied()
        .filter(|slot| slot.t_start < deadline && slot.t_start >= now.saturating_sub(900))
        .map(|slot| ChargeCandidate {
            t_end: slot.t_end.min(deadline),
            ..slot
        })
        .filter(|slot| slot.t_end > slot.t_start)
        .collect();
    // A carried ceiling is only a floor: a genuinely cheaper publication can still lower it.
    let threshold = price_threshold(&in_window)?.max(band_floor);
    let priced_without_headroom = in_window
        .iter()
        .any(|slot| slot.price_eur_per_kwh <= threshold && slot.available_w <= 0.0);
    let mut eligible: Vec<ChargeCandidate> = in_window
        .into_iter()
        .filter(|slot| slot.price_eur_per_kwh <= threshold && slot.available_w > 0.0)
        .collect();
    eligible.sort_by(|a, b| {
        a.t_start
            .cmp(&b.t_start)
            .then(a.price_eur_per_kwh.total_cmp(&b.price_eur_per_kwh))
    });
    let remaining = eligible
        .iter()
        .map(|slot| slot.available_w.min(budget_w).max(0.0))
        .collect();
    Some((eligible, remaining, priced_without_headroom, threshold))
}

/// A cheap slot is in the lower half of the overnight window *and* below its mean price.
///
/// The mean bound matters when a short dip sits next to a long expensive stretch: a plain median
/// would classify the expensive stretch as cheap simply because it occupies more than half the
/// window, and the planner would charge at the very price it was meant to avoid.
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

/// Complete the pack from the earliest run, or book the largest partial run available.
fn assign(
    pack: &DuePack,
    eligible: &[ChargeCandidate],
    remaining: &mut [f64],
    settings: &FullChargeSettings,
    now: i64,
    band_ceil: f64,
) -> Option<FullChargeAssignment> {
    let needed = pack.ac_energy_needed_kwh();
    if needed <= ENERGY_EPS_KWH {
        return Some(assignment(pack, settings, Vec::new(), 0.0, now, band_ceil));
    }
    if !pack.max_charge_w.is_finite() || pack.max_charge_w <= 0.0 {
        return None;
    }
    // Starting at the earliest slot both finds the earliest complete window and, when none can
    // complete, gathers the most energy the remaining cheap band can still deliver.
    let (reserved, cost, delivered, complete) = fill(pack, eligible, remaining, 0, needed);
    if delivered <= ENERGY_EPS_KWH {
        return None;
    }
    let band_kwh = nominal_available(pack, eligible, remaining);
    let partial_reason = (!complete).then(|| shortfall_reason(band_kwh, needed));
    // Charge by slot index, not by vector offset: windows skip exhausted slots, and a shifted
    // index would hand the same headroom to the next pack twice.
    for (index, slot) in &reserved {
        remaining[*index] -= slot.charge_w;
    }
    let charge_end = reserved.last().map_or(now, |(_, slot)| slot.t_end);
    let slots = reserved.into_iter().map(|(_, slot)| slot).collect();
    let mut result = assignment(pack, settings, slots, cost, charge_end, band_ceil);
    if let Some(reason) = partial_reason {
        // The shortfall is against the band's full-quarter-hour capacity, not the seconds left
        // in the current slot: elapsed time is not energy that became unplannable.
        result.missing_kwh = (needed - band_kwh).max(0.0);
        result.partial_reason = Some(reason);
    }
    Some(result)
}

/// The assignment's bookkeeping fields, shared by the zero-charge and charged paths.
fn assignment(
    pack: &DuePack,
    settings: &FullChargeSettings,
    slots: Vec<ChargeSlot>,
    cost: f64,
    charge_end: i64,
    band_ceil: f64,
) -> FullChargeAssignment {
    FullChargeAssignment {
        pack_id: pack.pack_id.clone(),
        reason: if pack.forced {
            "manual_request"
        } else {
            "interval_due"
        },
        target_soc_pct: 100.0,
        slots,
        charge_end,
        dwell_until: charge_end + i64::from(settings.dwell_minutes()) * 60,
        expected_cost_eur: cost,
        missing_kwh: 0.0,
        partial_reason: None,
        band_ceil_eur_per_kwh: band_ceil,
    }
}

/// Next 07:00 in `Europe/Amsterdam`, strictly after `now`; the overnight balancing deadline.
fn deadline_after(now: i64) -> i64 {
    let Some(local) = DateTime::from_timestamp(now, 0).map(|at| at.with_timezone(&Amsterdam))
    else {
        return now.saturating_add(86_400);
    };
    let mut date = local.date_naive();
    if local.hour() >= DEADLINE_LOCAL_HOUR {
        date = date.succ_opt().unwrap_or(date);
    }
    for _ in 0..3 {
        let Some(naive) = date.and_hms_opt(DEADLINE_LOCAL_HOUR, 0, 0) else {
            break;
        };
        match Amsterdam.from_local_datetime(&naive) {
            LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => return at.timestamp(),
            LocalResult::None => date = date.succ_opt().unwrap_or(date),
        }
    }
    now.saturating_add(86_400)
}
