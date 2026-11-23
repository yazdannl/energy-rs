//! Plan-owned energy and money totals, computed from the final timeline once.

use crate::horizon::NetMeteringSchedule;
use crate::plan_assemble::TimelineSlot;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanSummary {
    /// PV energy that went into the battery, kWh.
    pub pv_charge_kwh: f64,
    /// Grid energy that went into the battery, kWh.
    pub grid_charge_kwh: f64,
    /// Total charged energy, kWh.
    pub charged_kwh: f64,
    /// Total discharged energy, kWh.
    pub discharged_kwh: f64,
    /// Plan grid cost, EUR.
    pub cost_eur: f64,
    /// No-battery baseline grid cost, EUR.
    pub baseline_eur: f64,
    /// Baseline minus plan cost, EUR.
    pub saved_eur: f64,
    /// Equivalent full cycles over the learned capacity.
    pub cycles: f64,
}

/// Summarise the plan path; import pays buy, export earns the slot's signed export price.
pub fn summarize(
    slots: &[TimelineSlot],
    schedule: &NetMeteringSchedule,
    capacity_kwh: f64,
    slot_seconds: i64,
) -> PlanSummary {
    let mut summary = PlanSummary::default();
    let hours = slot_seconds as f64 / 3600.0;
    for slot in slots {
        let (buy, export) = prices(schedule, slot.t_start);
        let base = slot.load_w - slot.pv_w;
        summary.cost_eur += grid_cost(slot.grid_w, buy, export, hours);
        summary.baseline_eur += grid_cost(base, buy, export, hours);
        if slot.battery_w > 0.0 {
            let charge = slot.battery_w * hours / 1000.0;
            let surplus = (slot.pv_w - slot.load_w).max(0.0);
            let from_pv = slot.battery_w.min(surplus) * hours / 1000.0;
            summary.charged_kwh += charge;
            summary.pv_charge_kwh += from_pv;
            summary.grid_charge_kwh += charge - from_pv;
        } else {
            summary.discharged_kwh += -slot.battery_w * hours / 1000.0;
        }
    }
    summary.saved_eur = summary.baseline_eur - summary.cost_eur;
    if capacity_kwh > 0.0 {
        summary.cycles = summary.discharged_kwh / capacity_kwh;
    }
    summary
}

fn prices(schedule: &NetMeteringSchedule, t_start: i64) -> (f64, f64) {
    schedule.at(t_start).map_or((0.0, 0.0), |row| {
        (row.buy_eur_per_kwh, row.export_eur_per_kwh)
    })
}

fn grid_cost(grid_w: f64, buy: f64, export: f64, hours: f64) -> f64 {
    if grid_w >= 0.0 {
        buy * grid_w * hours / 1000.0
    } else {
        export * grid_w * hours / 1000.0
    }
}
