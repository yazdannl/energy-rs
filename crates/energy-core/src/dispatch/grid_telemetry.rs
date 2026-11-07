//! The battery-idle grid base and whether the fleet check may use it.

use energy_ledger::plan::PlanSlot;

use crate::dispatch::TickInput;
use crate::dispatch::fleet_grid::GridBase;
use crate::safety::SafetyLimits;

/// The battery-idle grid base the fleet check bounds against.
///
/// The measured meter already contains the batteries' own action, so it is corrected back to idle
/// (`grid_w - fleet_power_w`) before it is used; the plan's `load - pv` is the fallback. Import
/// takes the larger base and export the smaller, so the check never relies on the optimistic
/// number. Missing telemetry, or a stale fleet reading with a site cap configured, yields `None`
/// and the check fails closed.
pub(crate) fn base(
    input: &TickInput<'_>,
    slot: Option<&PlanSlot>,
    limits: &SafetyLimits,
) -> Option<GridBase> {
    let caps = limits.max_grid_import_w > 0.0 || limits.max_grid_export_w > 0.0;
    let stale = input.fleet.packs.iter().any(|pack| pack.stale);
    if caps && stale {
        return None;
    }
    let forecast = slot
        .map(|slot| slot.load_kw - slot.pv_kw)
        .or_else(|| input.meter.base_kw())
        .filter(|value| value.is_finite());
    let measured = measured(input, stale);
    match (measured, forecast) {
        (Some(measured), Some(forecast)) => Some(GridBase {
            import_kw: measured.max(forecast),
            export_kw: measured.min(forecast),
        }),
        (Some(measured), None) => Some(GridBase {
            import_kw: measured,
            export_kw: measured,
        }),
        (None, Some(forecast)) => Some(GridBase {
            import_kw: forecast,
            export_kw: forecast,
        }),
        (None, None) => None,
    }
}

/// The measured grid flow corrected back to the battery-idle base, in kW.
fn measured(input: &TickInput<'_>, stale: bool) -> Option<f64> {
    if stale {
        return None;
    }
    let grid_w = input.meter.grid_w.filter(|value| value.is_finite())?;
    let base_kw = (grid_w - input.fleet.power_w()) / 1000.0;
    base_kw.is_finite().then_some(base_kw)
}
