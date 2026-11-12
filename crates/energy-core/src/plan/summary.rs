//! Plan money and energy totals, computed from the solver path only.

use energy_ledger::plan::PlanSlot;

use crate::plan::{PlanSummary, SLOT_MINUTES};

/// Sum the plan's grid cost and battery throughput.
pub(crate) fn summarize(
    timeline: &[PlanSlot],
    capacity_kwh: f64,
    terminal_target_kwh: Option<f64>,
) -> PlanSummary {
    let hours = SLOT_MINUTES / 60.0;
    let mut summary = PlanSummary {
        cost_estimate_eur: 0.0,
        baseline_cost_eur: 0.0,
        saved_eur: 0.0,
        charged_kwh: 0.0,
        discharged_kwh: 0.0,
        grid_charge_kwh: 0.0,
        pv_charge_kwh: 0.0,
        cycles: 0.0,
        terminal_target_kwh,
        infeasible_reason: None,
        export_floor_eur_per_kwh: None,
        zero_export: false,
        cost_basis_eur_per_kwh: None,
        peak_reserve_kwh: None,
        solar_reserve_kwh: 0.0,
        clipped_kwh: 0.0,
        gated_kwh: 0.0,
    };
    for slot in timeline {
        let base = slot.load_kw - slot.pv_kw;
        summary.cost_estimate_eur += cost(slot, base + slot.battery_kw, hours);
        summary.baseline_cost_eur += cost(slot, base, hours);
        if slot.battery_kw > 0.0 {
            let charge = slot.battery_kw * hours;
            summary.charged_kwh += charge;
            let from_grid = (slot.battery_kw - base.max(0.0)).max(0.0) * hours;
            summary.grid_charge_kwh += from_grid;
        } else {
            summary.discharged_kwh += -slot.battery_kw * hours;
        }
    }
    summary.pv_charge_kwh = (summary.charged_kwh - summary.grid_charge_kwh).max(0.0);
    summary.saved_eur = summary.baseline_cost_eur - summary.cost_estimate_eur;
    if capacity_kwh > 0.0 {
        summary.cycles = summary.discharged_kwh / capacity_kwh;
    }
    summary
}

/// One slot's grid cost: import pays `buy`, export earns the recorded feed-in.
///
/// An export is *negative* kWh at `feed_in`, so the cost term is negative too. Negating it as well
/// (the 0.2–0.8 versions did) charged the plan for every exported kWh and turned a profitable plan
/// into a loss on the summary: 120 slots of a real plan added up to +2.56 EUR and the summary
/// reported −5.82.
fn cost(slot: &PlanSlot, grid_kw: f64, hours: f64) -> f64 {
    if grid_kw >= 0.0 {
        slot.buy_all_in * grid_kw * hours
    } else {
        slot.feed_in * grid_kw * hours
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::summarize;
    use energy_ledger::plan::PlanSlot;

    fn slot(idx: i64, load: f64, pv: f64, battery: f64, buy: f64, feed_in: f64) -> PlanSlot {
        let mut slot = PlanSlot::at(idx, 1_800_000_000 + idx * 900);
        slot.load_kw = load;
        slot.pv_kw = pv;
        slot.battery_kw = battery;
        slot.buy_all_in = buy;
        slot.feed_in = feed_in;
        slot.soc_kwh = 1.0;
        slot
    }

    #[test]
    fn an_export_credits_the_plan_instead_of_charging_it() {
        // One slot imports 1 kWh, one exports 1 kWh at the same price: the plan is a wash.
        let slots = vec![
            slot(0, 1.0, 0.0, 0.0, 0.40, 0.40),
            slot(1, 0.0, 0.0, -1.0, 0.40, 0.40),
        ];
        let summary = summarize(&slots, 4.0, None);
        assert!(
            (summary.cost_estimate_eur).abs() < 1e-9,
            "{}",
            summary.cost_estimate_eur
        );
        assert!((summary.baseline_cost_eur - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_plan_that_sells_above_its_import_saves_money() {
        // Charge 1 kWh at 0.30, sell it at 0.60: the plan costs less than doing nothing.
        let slots = vec![
            slot(0, 0.0, 0.0, 1.0, 0.30, 0.30),
            slot(1, 0.0, 0.0, -1.0, 0.60, 0.60),
        ];
        let summary = summarize(&slots, 4.0, None);
        assert!(
            (summary.cost_estimate_eur + 0.075).abs() < 1e-9,
            "{}",
            summary.cost_estimate_eur
        );
        assert!(summary.saved_eur > 0.0);
    }
}
