//! The plan setpoint for the slot in force, interpolated over the slot boundary.

use crate::dispatch::TRANSITION_SECONDS;
use crate::plan::{Plan, SLOT_SECONDS};

/// The plan's setpoint at `now`, W (positive charging).
///
/// A slot starts at its modelled power, which is a step the hardware feels. Over the first
/// [`TRANSITION_SECONDS`] the setpoint therefore ramps linearly from the previous slot's value, so a
/// charge-to-discharge transition is a slope instead of a bang (survey §7 trap 17). `None` means the
/// plan no longer covers `now`, which the caller turns into a hold -- never into the last setpoint.
pub fn slot_power(plan: &Plan, now: i64) -> Option<f64> {
    let slot = plan.slot_at(now)?;
    let power = slot.battery_kw * 1000.0;
    let elapsed = now.saturating_sub(slot.t_start);
    if !(0..=TRANSITION_SECONDS).contains(&elapsed) {
        return Some(power);
    }
    let previous = previous_slot(plan, slot.t_start).map(|slot| slot.battery_kw * 1000.0);
    match previous {
        Some(previous) if (previous - power).abs() > 1e-6 => {
            let fraction = elapsed as f64 / TRANSITION_SECONDS as f64;
            Some(previous + (power - previous) * fraction)
        }
        _ => Some(power),
    }
}

fn previous_slot(plan: &Plan, t_start: i64) -> Option<&energy_ledger::plan::PlanSlot> {
    let wanted = t_start.checked_sub(SLOT_SECONDS)?;
    plan.slots.iter().find(|slot| slot.t_start == wanted)
}
