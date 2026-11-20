//! The plan's maintenance slots, turned into per-pack overrides for the dispatcher.

use energy_battery::protection::full_charge::TOLERANCE_PCT;

use crate::dispatch::{FleetView, ManualOverride, OverrideAction};
use crate::plan::{Plan, SLOT_SECONDS};

/// How long one maintenance override outlives its tick, seconds.
const OVERRIDE_TTL_S: i64 = 60;
/// A run that has not reached 100 % two hours after its last cheap slot is abandoned.
const CHARGE_GRACE_S: i64 = 2 * 3600;

/// Maintenance overrides for this tick, in plan order.
///
/// The economic plan already contains the maintenance energy; these overrides make the *named*
/// packs take it instead of letting the fleet split spread it over packs that are not due. The
/// dispatcher's ladder still applies per-pack limits, the cell guard and the import ceiling.
pub fn overrides(plan: &Plan, view: &FleetView, now: i64) -> Vec<ManualOverride> {
    let until = now.saturating_add(OVERRIDE_TTL_S);
    let mut entries = Vec::new();
    for assignment in &plan.maintenance {
        let Some(pack) = view
            .packs
            .iter()
            .find(|pack| pack.pack_id() == assignment.pack_id)
        else {
            continue;
        };
        if now >= assignment.charge_end.saturating_add(CHARGE_GRACE_S) {
            continue;
        }
        let action = if now < assignment.charge_end {
            charge_in_slot(assignment, now, pack.spec.max_charge_w)
        } else if reached(pack, assignment.target_soc_pct) {
            OverrideAction::Hold
        } else {
            OverrideAction::Charge {
                watts: pack.spec.max_charge_w,
            }
        };
        entries.push(ManualOverride::maintenance(
            assignment.pack_id.clone(),
            action,
            until,
        ));
    }
    entries
}

/// Charge only in the chosen cheap slot; hold the pack in the gaps between them.
fn charge_in_slot(
    assignment: &energy_battery::protection::full_charge::FullChargeAssignment,
    now: i64,
    fallback_w: f64,
) -> OverrideAction {
    assignment
        .slots
        .iter()
        .find(|slot| now >= slot.t_start && now < slot.t_start + SLOT_SECONDS)
        .map_or(OverrideAction::Off, |slot| OverrideAction::Charge {
            watts: slot.charge_w.max(0.0).min(fallback_w),
        })
}

/// Whether the pack proved it arrived at the target and can start balancing.
fn reached(pack: &crate::dispatch::PackReading, target: f64) -> bool {
    pack.soc_pct
        .is_some_and(|soc| soc.is_finite() && soc >= target - TOLERANCE_PCT)
}
