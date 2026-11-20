//! Turn the solved plan's horizon into import-bounded maintenance candidates.

use energy_battery::protection::full_charge::ChargeCandidate;

use crate::plan::{Plan, SLOT_SECONDS};

/// The quarter-hours a maintenance charge could use, cheapest first decided later.
///
/// The budget is the grid import ceiling *after* the configured margin. House import and any
/// charging the economic plan already scheduled are deducted up front, so folding maintenance
/// power into the same slots cannot push the site past the electrical limit.
pub fn candidates(plan: &Plan, now: i64, budget_w: f64) -> Vec<ChargeCandidate> {
    if !budget_w.is_finite() || budget_w <= 0.0 {
        return Vec::new();
    }
    plan.slots
        .iter()
        .filter(|slot| slot.t_end() > now)
        .filter(|slot| slot.t_end() <= plan.horizon_end)
        .filter(|slot| slot.buy_all_in.is_finite())
        .map(|slot| {
            let house_w = (slot.load_kw - slot.pv_kw).max(0.0) * 1000.0;
            let planned_w = slot.battery_kw.max(0.0) * 1000.0;
            // A replan late in a slot may only use the seconds that remain: crediting the full
            // quarter-hour would plan energy no device can deliver before the slot boundary.
            let t_start = slot.t_start.max(now);
            ChargeCandidate {
                slot_start: slot.t_start,
                t_start,
                t_end: slot.t_end(),
                price_eur_per_kwh: slot.buy_all_in,
                available_w: (budget_w - house_w - planned_w).max(0.0),
            }
        })
        .collect()
}

/// Exclusive end of a plan slot.
trait SlotEnd {
    fn t_end(&self) -> i64;
}

impl SlotEnd for energy_ledger::plan::PlanSlot {
    fn t_end(&self) -> i64 {
        self.t_start + SLOT_SECONDS
    }
}
