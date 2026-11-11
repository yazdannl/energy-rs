//! The plan setpoint corrected towards the measured grid, once per tick.
//!
//! The follow only ever holds the site at **zero** exchange. A slot that is deliberately exchanging
//! energy — charging from the grid, or selling — has already decided its battery rate, and that
//! rate is the decision: following the grid there would trade the plan's charge (or sale) for a
//! net-import target, which is not what the plan said. So those slots are left alone, and every
//! other slot is self-consumption: absorb the surplus, cover the house, export or import only what
//! the fleet physically cannot avoid.

use crate::dispatch::{FleetView, MeterReading};
use crate::safety::{FollowRequest, GridFollow, SafetyLimits};
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
    let Some(desired_grid_w) = desired_grid_w(slot) else {
        return plan_power_w;
    };
    let follow = GridFollow {
        deadband_w: limits.grid_follow_deadband_w,
        min_budget_w: limits.grid_follow_min_budget_w,
    };
    follow.correct(&FollowRequest {
        plan_w: plan_power_w,
        battery_w: fleet.power_w(),
        desired_grid_w,
        measured_grid_w: grid_w,
        capability_w: capability_w(fleet),
    })
}

/// The grid this slot should hold, W; `None` when the slot is deliberately exchanging energy.
///
/// A slot whose battery and grid flow point the same way is buying to charge or selling on purpose.
/// Its number is a *battery rate*, not a grid target, so the follow leaves it alone. Every other
/// slot's grid is an artefact of the load and PV the plan assumed, and the site holds the meter at
/// zero instead — absorb what the sun delivers, cover what the house draws, rather than importing
/// to match a forecast that turned out wrong.
fn desired_grid_w(slot: &PlanSlot) -> Option<f64> {
    if ![slot.load_kw, slot.pv_kw, slot.battery_kw]
        .iter()
        .all(|value| value.is_finite())
    {
        // A slot that cannot say what it expected is not a target: hold the plan and let the
        // ladder's missing-telemetry rule do its job.
        return None;
    }
    let planned_grid = slot.load_kw - slot.pv_kw - slot.battery_kw;
    let exchanging = slot.battery_kw * planned_grid > 0.0;
    (!exchanging).then_some(0.0)
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
