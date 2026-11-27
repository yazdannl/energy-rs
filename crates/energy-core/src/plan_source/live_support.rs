//! Pure helpers shared by the live plan source.

use energy_price::PriceBook;

use crate::plan::{HORIZON_SLOTS, PackState, PlanError, SLOT_SECONDS};
use crate::plan_source::{MIN_HORIZON_SLOTS, Supply};

/// Capacity-weighted charge efficiency for the plan's energy path.
pub(crate) fn weighted_charge_efficiency(packs: &[PackState]) -> f64 {
    let (weighted, capacity): (f64, f64) = packs.iter().fold((0.0, 0.0), |(energy, cap), pack| {
        (
            energy + pack.spec.eta_charge * pack.spec.capacity_kwh,
            cap + pack.spec.capacity_kwh,
        )
    });
    if capacity > 0.0 {
        weighted / capacity
    } else {
        1.0
    }
}

/// Why the provider forecast was not used.
pub(crate) fn fallback_note(supply: &Supply, slots: usize, now: i64, max_age_s: i64) -> String {
    match &supply.forecast {
        None => "no forecast available; EWMA fallback from measured history".to_owned(),
        Some(forecast) if forecast.len() < slots => format!(
            "forecast covers {} of {slots} slots; EWMA fallback",
            forecast.len()
        ),
        Some(forecast) => {
            let age = now.saturating_sub(forecast.as_of).max(0);
            format!("forecast is {age}s old (limit {max_age_s}s); EWMA fallback")
        }
    }
}

/// Contiguous quarter-hours the book covers from the slot containing `now`, capped at the horizon.
///
/// Never forward-fills: before the ~13:00 CET publication tomorrow's slots simply do not exist, so
/// the plan is shorter and the replan trigger replaces it once they do. A book that cannot cover
/// the next hour at all is refused as stale/insufficient instead of producing a one-slot plan.
pub(crate) fn horizon_slots(prices: &PriceBook, now: i64) -> Result<usize, PlanError> {
    let first = prices
        .slot_at(now)
        .ok_or(PlanError::PriceGap { t_start: now })?;
    if first.source.slot_seconds() != SLOT_SECONDS {
        return Err(PlanError::PriceResolution {
            expected_s: SLOT_SECONDS,
            got_s: first.source.slot_seconds(),
        });
    }
    let mut slots = 0;
    let mut t_start = first.t_start;
    while slots < HORIZON_SLOTS {
        let covered = prices
            .slot_at(t_start)
            .is_some_and(|slot| slot.t_start == t_start);
        if !covered {
            break;
        }
        slots += 1;
        t_start = t_start.saturating_add(SLOT_SECONDS);
    }
    if slots < MIN_HORIZON_SLOTS {
        return Err(PlanError::PriceCoverage {
            slots,
            minimum: MIN_HORIZON_SLOTS,
        });
    }
    Ok(slots)
}
