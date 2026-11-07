//! Validate the *final* command vector, after every per-pack bound has had its say.

use crate::allocation::ALLOCATION_DEADBAND_W;
use crate::dispatch::Planned;
use crate::dispatch::tick_frame::TickFrame;
use crate::safety::{Reason, SafetyLimits};

/// Values within this many watts are floating-point noise, not a change.
const EPS_W: f64 = 0.5;

/// Bound the summed command vector by the fleet power caps and the site import/export ceilings.
///
/// The allocation clamp runs *before* the per-pack ladder, so an override, a forced emergency charge
/// or a ramp can legitimately lift the final vector back over what the allocation allowed: four packs
/// each forced to 700 W are 2800 W on a 1000 W site limit. This pass therefore looks at what will
/// actually be written, adds the measured power of the packs the tick does not touch (they keep
/// executing their last command), and reduces the vector until it fits -- monotonically, keeping the
/// split proportional and never flipping a sign. A reduction a side cannot absorb (the untouched
/// packs alone already break the limit) removes that side entirely and says so.
pub(crate) fn apply(plans: &mut [Planned], frame: &TickFrame<'_>) -> Option<Reason> {
    let vector: Vec<usize> = (0..plans.len())
        .filter(|index| plans[*index].vector)
        .collect();
    if vector.is_empty() {
        return None;
    }
    let writing = frame.mode.may_write();
    let live = if writing { others(plans, &vector) } else { 0.0 };
    let live_charge = if writing {
        side(plans, &others_indices(&vector, plans.len()), true)
    } else {
        0.0
    };
    let live_discharge = if writing {
        side(plans, &others_indices(&vector, plans.len()), false)
    } else {
        0.0
    };
    let mut reason = grid(plans, &vector, frame, live);
    if reason.is_none() {
        reason = power(plans, &vector, frame.limits, (live_charge, live_discharge));
    }
    reason
}

/// The site ceiling, with the untouched packs' measured power counted in.
fn grid(
    plans: &mut [Planned],
    vector: &[usize],
    frame: &TickFrame<'_>,
    live: f64,
) -> Option<Reason> {
    let base = frame.grid_base.as_ref()?;
    let mut reason = None;
    let import = frame.limits.max_grid_import_w;
    if import > 0.0 {
        let allowed = import - base.import_kw * 1000.0 - live;
        let excess = total(plans, vector) - allowed;
        if reduce(plans, vector, true, excess, Reason::GridImportLimit) > EPS_W {
            reason = Some(Reason::GridImportLimit);
        }
    }
    let export = frame.limits.max_grid_export_w;
    if export > 0.0 {
        let allowed = -(export + base.export_kw * 1000.0) - live;
        let excess = allowed - total(plans, vector);
        if reduce(plans, vector, false, excess, Reason::GridExportLimit) > EPS_W {
            reason = Some(Reason::GridExportLimit);
        }
    }
    reason
}

/// The operator's fleet charge/discharge caps, applied to the same final vector.
///
/// `live` is the untouched packs' measured power on each side `(charge, discharge)`: a pack that is
/// still executing 500 W is part of the fleet's 1000 W charge budget even though this tick does not
/// command it.
fn power(
    plans: &mut [Planned],
    vector: &[usize],
    limits: &SafetyLimits,
    live: (f64, f64),
) -> Option<Reason> {
    let mut reason = None;
    if limits.fleet_max_charge_w > 0.0 {
        let excess = side(plans, vector, true) - (limits.fleet_max_charge_w - live.0);
        if reduce(plans, vector, true, excess, Reason::PowerClamp) > EPS_W {
            reason = Some(Reason::PowerClamp);
        }
    }
    if limits.fleet_max_discharge_w > 0.0 {
        let excess = side(plans, vector, false) - (limits.fleet_max_discharge_w - live.1);
        if reduce(plans, vector, false, excess, Reason::PowerClamp) > EPS_W {
            reason = Some(Reason::PowerClamp);
        }
    }
    reason
}

/// The signed sum of the commands the tick would write.
fn total(plans: &[Planned], vector: &[usize]) -> f64 {
    vector.iter().map(|index| plans[*index].command_w).sum()
}

/// The summed magnitude on one side of the vector.
fn side(plans: &[Planned], vector: &[usize], charging: bool) -> f64 {
    vector
        .iter()
        .map(|index| part(plans[*index].command_w, charging))
        .sum()
}

/// The indices this tick does not put in the command vector.
fn others_indices(vector: &[usize], len: usize) -> Vec<usize> {
    (0..len).filter(|index| !vector.contains(index)).collect()
}

/// The summed measured power of the packs this tick does not command, in watts.
fn others(plans: &[Planned], vector: &[usize]) -> f64 {
    others_indices(vector, plans.len())
        .into_iter()
        .map(|index| plans[index].measured_w)
        .sum()
}

/// Remove up to `deficit` watts from one side of the vector; returns what was actually removed.
fn reduce(
    plans: &mut [Planned],
    vector: &[usize],
    charging: bool,
    deficit: f64,
    reason: Reason,
) -> f64 {
    if !deficit.is_finite() || deficit <= EPS_W {
        return 0.0;
    }
    let available = side(plans, vector, charging);
    if available <= EPS_W {
        return 0.0;
    }
    let keep = ((available - deficit) / available).clamp(0.0, 1.0);
    let mut removed = 0.0;
    for &index in vector {
        let current = plans[index].command_w;
        let magnitude = part(current, charging);
        if magnitude <= EPS_W {
            continue;
        }
        let toward_zero = magnitude * (1.0 - keep);
        let next = if charging {
            current - toward_zero
        } else {
            current + toward_zero
        };
        let next = if next.abs() <= ALLOCATION_DEADBAND_W {
            0.0
        } else {
            next
        };
        removed += magnitude - part(next, charging);
        plans[index].command_w = next;
        plans[index].reason = reason;
    }
    removed
}

/// How much of `watts` is on the requested side.
fn part(watts: f64, charging: bool) -> f64 {
    if charging {
        watts.max(0.0)
    } else {
        (-watts).max(0.0)
    }
}
