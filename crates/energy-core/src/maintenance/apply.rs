//! Fold chosen maintenance charges into the economic plan's timeline.

use energy_battery::protection::full_charge::FullChargeAssignment;

use crate::plan::{Plan, SLOT_MINUTES, SLOT_SECONDS, summary, timeline};

/// Add the maintenance power to its slots and rebuild the SoC path and money totals.
///
/// The assignment's grid headroom was computed against this plan's own charging, so the resulting
/// `battery_kw` is already inside the import ceiling. Re-summarising is what makes the charge show
/// up in `chargedWh`/`gridChargeWh` and in the stored plan the API reads.
pub fn apply(
    plan: &mut Plan,
    assignments: &[FullChargeAssignment],
    capacity_kwh: f64,
    eta_charge: f64,
) {
    if assignments.is_empty() {
        return;
    }
    for assignment in assignments {
        for charge in &assignment.slots {
            let Some(slot) = plan
                .slots
                .iter_mut()
                .find(|row| row.t_start == charge.slot_start)
            else {
                continue;
            };
            // Convert actual charging seconds into the slot's average power; a clipped first or
            // last slot then contributes only the energy that can really be delivered.
            let average_w = charge.charge_w * charge.duration_s as f64 / SLOT_SECONDS as f64;
            if slot.battery_kw < 0.0 {
                // A maintenance charge replaces a conflicting discharge; adding would cancel it
                // twice and understate both the grid import and the energy put into the pack.
                slot.battery_kw = average_w / 1000.0;
            } else {
                slot.battery_kw += average_w / 1000.0;
            }
            if !slot.reason.is_empty() {
                slot.reason.push_str("; ");
            }
            slot.reason.push_str("maintenance_full_charge");
        }
    }
    let eta = if eta_charge.is_finite() && eta_charge > 0.0 {
        eta_charge
    } else {
        1.0
    };
    let mut energy = plan.slots.first().map_or(0.0, |slot| slot.soc_kwh);
    for slot in &mut plan.slots {
        slot.soc_kwh = energy;
        energy = timeline::end_energy(energy, slot.battery_kw, SLOT_MINUTES / 60.0, eta, 1.0);
        slot.target_soc_kwh = energy;
    }
    let previous = plan.summary.clone();
    let mut next = summary::summarize(&plan.slots, capacity_kwh, previous.terminal_target_kwh);
    next.export_floor_eur_per_kwh = previous.export_floor_eur_per_kwh;
    next.zero_export = previous.zero_export;
    next.cost_basis_eur_per_kwh = previous.cost_basis_eur_per_kwh;
    next.peak_reserve_kwh = previous.peak_reserve_kwh;
    next.solar_reserve_kwh = previous.solar_reserve_kwh;
    next.clipped_kwh = previous.clipped_kwh;
    next.gated_kwh = previous.gated_kwh;
    plan.objective_eur = next.cost_estimate_eur;
    plan.summary = next;
}
