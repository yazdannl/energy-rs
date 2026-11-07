//! The fleet-level grid clamp, applied after the per-pack split.

use crate::allocation::ALLOCATION_DEADBAND_W;
use crate::allocation::Allocation;
use crate::safety::{Reason, SafetyLimits};

/// The battery-idle grid base, in kW, for both directions.
///
/// `import_kw` and `export_kw` may differ because the base is the conservative combination of the
/// measured flow (corrected back to battery-idle) and the forecast: the site limit must bind on the
/// side that actually binds, so an import check takes the larger base and an export check the
/// smaller. The dispatcher supplies `None` when the telemetry is missing or stale, which fails the
/// check closed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridBase {
    pub(crate) import_kw: f64,
    pub(crate) export_kw: f64,
}

/// Bound the fleet's summed setpoint by the site's import/export ceiling.
///
/// The per-pack ladder already bounds each pack against the grid, but with four packs the sum of
/// four individually legal setpoints can still exceed the site limit; this is the aggregate check
/// the engine runs *after* allocation. It scales every share by the same factor, so the split stays
/// proportional, and returns the binding reason for the tick. With a cap configured and no usable
/// base, the whole allocation is zeroed (`grid_telemetry_missing`), never guessed.
pub(crate) fn clamp(
    allocation: &mut Allocation,
    base: Option<&GridBase>,
    limits: &SafetyLimits,
) -> Option<Reason> {
    let total = allocation.allocated_w;
    if total.abs() <= ALLOCATION_DEADBAND_W {
        return None;
    }
    if total > 0.0 && limits.max_grid_import_w > 0.0 {
        return bound_import(allocation, base, limits.max_grid_import_w);
    }
    if total < 0.0 && limits.max_grid_export_w > 0.0 {
        return bound_export(allocation, base, limits.max_grid_export_w);
    }
    None
}

fn bound_import(
    allocation: &mut Allocation,
    base: Option<&GridBase>,
    limit_w: f64,
) -> Option<Reason> {
    let Some(base) = base else {
        zero(allocation);
        return Some(Reason::GridTelemetryMissing);
    };
    let grid_w = base.import_kw * 1000.0 + allocation.allocated_w;
    if grid_w <= limit_w {
        return None;
    }
    let allowed = limit_w - base.import_kw * 1000.0;
    rescale(allocation, allowed);
    Some(Reason::GridImportLimit)
}

fn bound_export(
    allocation: &mut Allocation,
    base: Option<&GridBase>,
    limit_w: f64,
) -> Option<Reason> {
    let Some(base) = base else {
        zero(allocation);
        return Some(Reason::GridTelemetryMissing);
    };
    let grid_w = base.export_kw * 1000.0 + allocation.allocated_w;
    if grid_w >= -limit_w {
        return None;
    }
    let allowed = -(limit_w + base.export_kw * 1000.0);
    rescale(allocation, allowed);
    Some(Reason::GridExportLimit)
}

/// Scale every pack's share so the fleet total becomes `allowed_w`, keeping the sign.
fn rescale(allocation: &mut Allocation, allowed_w: f64) {
    let total = allocation.allocated_w;
    if total.abs() <= ALLOCATION_DEADBAND_W {
        return;
    }
    let capped = if total > 0.0 {
        allowed_w.clamp(0.0, total)
    } else {
        allowed_w.clamp(total, 0.0)
    };
    let factor = capped / total;
    for pack in &mut allocation.packs {
        pack.watts *= factor;
    }
    allocation.allocated_w = allocation.packs.iter().map(|pack| pack.watts).sum();
    allocation.unmet_w = allocation.requested_w - allocation.allocated_w;
}

/// Drop the whole allocation: the site bound cannot be evaluated, so nothing is written.
fn zero(allocation: &mut Allocation) {
    for pack in &mut allocation.packs {
        pack.watts = 0.0;
    }
    allocation.allocated_w = 0.0;
    allocation.unmet_w = allocation.requested_w;
}
