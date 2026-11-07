//! The weekly full-charge maintenance window, turned into expiring overrides.

use energy_battery::protection::full_charge::{
    FullChargeAction, FullChargeScheduler, TOLERANCE_PCT,
};

use crate::dispatch::{FleetView, ManualOverride, OverrideAction};
use crate::engine::instant;
use crate::wiring::watts_u32;

/// How long a maintenance override outlives the tick that created it, seconds.
///
/// The overrides are rebuilt on every tick, so this only has to bridge one tick; the margin covers
/// a slow device read without leaving a maintenance charge running after the window closes.
const OVERRIDE_TTL_S: i64 = 60;

/// Ask the scheduler what maintenance wants now.
///
/// `ceiling_w` is the fleet's own charge ceiling, so the answer is already inside what the hardware
/// can take; the ladder clamps it again per pack. A missing SoC counts as "not at the target": the
/// pack has to prove it arrived.
pub(crate) fn consult(
    scheduler: &mut FullChargeScheduler,
    view: &FleetView,
    now: i64,
) -> FullChargeAction {
    scheduler.evaluate(instant(now), view.soc_pct(), watts_u32(view.max_charge_w()))
}

/// Turn a maintenance action into per-pack overrides (operator overrides are appended after these).
pub(crate) fn overrides(
    action: FullChargeAction,
    view: &FleetView,
    now: i64,
) -> Vec<ManualOverride> {
    let until = now.saturating_add(OVERRIDE_TTL_S);
    match action {
        FullChargeAction::Idle | FullChargeAction::Release => Vec::new(),
        FullChargeAction::Hold { .. } => view
            .packs
            .iter()
            .map(|pack| ManualOverride::maintenance(pack.pack_id(), OverrideAction::Hold, until))
            .collect(),
        FullChargeAction::Charge {
            limit_w,
            target_soc_pct,
        } => {
            let count = view.packs.len();
            if count == 0 {
                return Vec::new();
            }
            let watts = f64::from(limit_w) / count as f64;
            view.packs
                .iter()
                .map(|pack| {
                    let reached = pack
                        .soc_pct
                        .is_some_and(|soc| soc >= target_soc_pct - TOLERANCE_PCT);
                    let action = if reached {
                        OverrideAction::Hold
                    } else {
                        OverrideAction::Charge { watts }
                    };
                    ManualOverride::maintenance(pack.pack_id(), action, until)
                })
                .collect()
        }
    }
}
