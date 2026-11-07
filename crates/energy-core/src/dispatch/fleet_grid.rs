//! The fleet-level grid clamp, applied after the per-pack split.

use crate::allocation::ALLOCATION_DEADBAND_W;
use crate::allocation::Allocation;
use crate::safety::{Reason, SafetyLimits};

/// Bound the fleet's summed setpoint by the site's import/export ceiling.
///
/// The per-pack ladder already bounds each pack against the grid, but with four packs the sum of
/// four individually legal setpoints can still exceed the site limit; this is the aggregate check
/// the engine runs *after* allocation. It scales every share by the same factor, so the split stays
/// proportional, and returns the binding reason for the tick.
pub(crate) fn clamp(
    allocation: &mut Allocation,
    base_kw: Option<f64>,
    limits: &SafetyLimits,
) -> Option<Reason> {
    let base = base_kw.filter(|value| value.is_finite())?;
    let total = allocation.allocated_w;
    if total.abs() <= ALLOCATION_DEADBAND_W {
        return None;
    }
    let grid_w = base * 1000.0 + total;
    if limits.max_grid_import_w > 0.0 && grid_w > limits.max_grid_import_w {
        let allowed = limits.max_grid_import_w - base * 1000.0;
        rescale(allocation, allowed);
        return Some(Reason::GridImportLimit);
    }
    if limits.max_grid_export_w > 0.0 && grid_w < -limits.max_grid_export_w {
        let allowed = -(limits.max_grid_export_w + base * 1000.0);
        rescale(allocation, allowed);
        return Some(Reason::GridExportLimit);
    }
    None
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
