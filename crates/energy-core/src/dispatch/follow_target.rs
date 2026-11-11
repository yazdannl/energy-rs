//! The plan setpoint corrected towards the measured grid, once per tick.

use crate::dispatch::{FleetView, MeterReading};
use crate::safety::{GridFollow, SafetyLimits};
use energy_ledger::plan::PlanSlot;

/// The target the fleet is actually asked for: the plan's setpoint, plus the grid-follow move.
///
/// The loop is a pure correction, not a plan change: the plan says whether this hour charges or
/// serves and how much, and the correction closes whatever the meter says the house is doing right
/// now. It is skipped when there is nothing to follow — a stale tick, no slot, or no grid reading —
/// because a follow without an intent is a guess.
pub(crate) fn target(
    plan_power_w: f64,
    slot: Option<&PlanSlot>,
    meter: &MeterReading,
    fleet: &FleetView,
    limits: &SafetyLimits,
    stale: bool,
) -> f64 {
    if stale {
        return plan_power_w;
    }
    let (Some(slot), Some(grid_w)) = (slot, meter.grid_w) else {
        return plan_power_w;
    };
    let follow = GridFollow {
        deadband_w: limits.grid_follow_deadband_w,
        min_budget_w: limits.grid_follow_min_budget_w,
    };
    follow.correct(
        plan_power_w,
        grid_w,
        desired_grid_kw(slot),
        capability_w(fleet),
    )
}

/// The grid this slot is *deliberately* making the meter read, kW.
///
/// The slot's own grid (`load − pv − battery`) is only a decision when the plan is exchanging energy
/// on purpose: charging from the grid (battery and grid both importing) or selling (both
/// exporting). Everywhere else the slot's grid is an artefact of the load and PV it assumed, and
/// the site should hold the meter at zero instead — absorb what the sun delivers and cover what the
/// house draws, rather than importing to match a forecast that turned out wrong.
fn desired_grid_kw(slot: &PlanSlot) -> f64 {
    let planned_grid = slot.load_kw - slot.pv_kw - slot.battery_kw;
    let exchanging = slot.battery_kw * planned_grid > 0.0;
    if exchanging { planned_grid } else { 0.0 }
}

/// The fleet's AC capability, W: what the packs together could take or deliver.
fn capability_w(fleet: &FleetView) -> f64 {
    fleet
        .packs
        .iter()
        .map(|pack| pack.spec.max_charge_w.max(pack.spec.max_discharge_w))
        .filter(|watts| watts.is_finite() && *watts > 0.0)
        .sum()
}
