//! Turn the solver's path into the ledger's `plan_slots` rows.

use energy_ledger::plan::PlanSlot;
use energy_opt::SolveResult;
use energy_price::{ExportSource, Settlement, Slot};

use crate::plan::{PlanError, PlanInput, SLOT_MINUTES};

/// One ledger slot per solver slot: prices, forecasts, power and the SoC path.
pub(crate) fn timeline(
    input: &PlanInput,
    slots: &[Slot],
    solved: &SolveResult,
    etas: (f64, f64),
) -> Result<Vec<PlanSlot>, PlanError> {
    if solved.energies.len() != slots.len() || solved.powers.len() != slots.len() {
        return Err(PlanError::SolveShape {
            expected: slots.len(),
            got: solved.energies.len().min(solved.powers.len()),
        });
    }
    let hours = SLOT_MINUTES / 60.0;
    let (eta_charge, eta_discharge) = etas;
    let mut timeline = Vec::with_capacity(slots.len());
    for (index, (slot, (energy, power))) in slots
        .iter()
        .zip(solved.energies.iter().zip(solved.powers.iter()))
        .enumerate()
    {
        // The timeline length is the 120-slot horizon; cast is bounded.
        #[allow(clippy::cast_possible_wrap)]
        let idx = index as i64;
        let mut row = PlanSlot::at(idx, slot.t_start);
        row.buy_all_in = slot.buy_all_in;
        row.feed_in = input.settlement.export_price(slot, ExportSource::Battery);
        row.pv_kw = input.pv_kw[index];
        row.load_kw = input.load_kw[index];
        row.battery_kw = *power;
        row.soc_kwh = *energy;
        row.target_soc_kwh = solved
            .energies
            .get(index + 1)
            .copied()
            .unwrap_or_else(|| end_energy(*energy, *power, hours, eta_charge, eta_discharge));
        reason(&row).clone_into(&mut row.reason);
        timeline.push(row);
    }
    Ok(timeline)
}

/// Energy at the end of a slot; the DP reports starts, so the last slot needs its own step.
pub(crate) fn end_energy(
    energy_kwh: f64,
    power_kw: f64,
    hours: f64,
    eta_charge: f64,
    eta_discharge: f64,
) -> f64 {
    if power_kw > 0.0 {
        energy_kwh + power_kw * hours * eta_charge
    } else {
        energy_kwh + power_kw * hours / eta_discharge
    }
}

/// The stable slot reason slug the UI and the journal read.
fn reason(slot: &PlanSlot) -> &'static str {
    if slot.battery_kw > 1e-3 {
        if slot.pv_kw > slot.load_kw {
            "pv_surplus_charge"
        } else {
            "arbitrage_charge"
        }
    } else if slot.battery_kw < -1e-3 {
        if slot.load_kw - slot.pv_kw + slot.battery_kw < 0.0 {
            "peak_discharge"
        } else {
            "self_consumption"
        }
    } else {
        "reserve_hold"
    }
}

/// The export price the settlement would pay for battery energy in this slot.
pub(crate) fn feed_in(settlement: &Settlement, slot: &Slot) -> f64 {
    settlement.export_price(slot, ExportSource::Battery)
}
