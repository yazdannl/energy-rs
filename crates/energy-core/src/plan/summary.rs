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
fn cost(slot: &PlanSlot, grid_kw: f64, hours: f64) -> f64 {
    if grid_kw >= 0.0 {
        slot.buy_all_in * grid_kw * hours
    } else {
        -slot.feed_in * grid_kw * hours
    }
}
