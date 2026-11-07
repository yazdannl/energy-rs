//! The cell-voltage latch, fed from the fleet read.

use chrono::DateTime;
use chrono::Utc;
use energy_battery::protection::{CellConfig, CellLatchStore, CellReading, CellVoltageGuard};

use crate::dispatch::FleetView;
use crate::engine::instant;
use crate::wiring::error::WiringError;
use crate::wiring::watts_u32;

/// Evaluate every pack's low-cell latch and persist the ones that changed.
///
/// A missing or stale `min_cell_v` keeps the previous latch (the fail-safe direction); a fresh
/// reading below the cutoff blocks discharge and asks for an emergency charge. The returned ids are
/// the packs whose latch changed, which is the signal to journal the transition.
pub(crate) fn evaluate(
    store: &dyn CellLatchStore,
    guard: &mut CellVoltageGuard,
    view: &FleetView,
    config: &CellConfig,
    now: i64,
) -> Result<Vec<String>, WiringError> {
    let at: DateTime<Utc> = instant(now);
    let mut changed = Vec::new();
    for pack in &view.packs {
        let observed = instant(pack.observed_at);
        let reading = match pack
            .min_cell_v
            .filter(|volt| volt.is_finite() && *volt > 0.0)
        {
            Some(voltage) => CellReading::fresh(voltage, observed),
            None => CellReading::missing(observed),
        };
        let per_pack = CellConfig {
            cutoff_v: config.cutoff_v,
            resume_v: config.resume_v,
            stale_after_s: config.stale_after_s,
            max_charge_w: watts_u32(pack.spec.max_charge_w),
            emergency_power_w: config.emergency_power_w,
        };
        let decision = guard.evaluate(pack.pack_id(), &reading, &per_pack, at);
        if decision.changed {
            guard.save(store, pack.pack_id())?;
            changed.push(pack.pack_id().to_owned());
        }
    }
    Ok(changed)
}
