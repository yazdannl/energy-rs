//! Reconcile an obligation's claimed availability with the final maintained timeline.

use crate::plan_assemble::{Obligation, PlanInput, deadline_instant};

/// Stored energy delivered to the named pack before its deadline; absent rows deliver zero.
pub fn delivered_until(obligation: &Obligation, input: &PlanInput) -> f64 {
    let deadline = deadline_instant(
        &obligation.deadline,
        input.horizon.t0,
        input.horizon.coverage_end(),
    )
    .unwrap_or_else(|| input.horizon.coverage_end());
    let hours = input.bounds.slot_seconds as f64 / 3600.0;
    let mut delivered = 0.0;
    for slot in &input.slots {
        if slot.t_start >= deadline {
            continue;
        }
        for pack in &slot.pack_powers {
            if pack.pack_id == obligation.pack_id {
                delivered += stored_delta(pack.w, hours, &input.bounds);
            }
        }
    }
    let rounded = (delivered * 1_000_000_000.0).round() / 1_000_000_000.0;
    rounded.max(0.0)
}

fn stored_delta(watts: f64, hours: f64, bounds: &crate::plan_assemble::Bounds) -> f64 {
    if watts >= 0.0 {
        watts * hours / 1000.0 * bounds.eta_charge
    } else {
        watts * hours / 1000.0 / bounds.eta_discharge
    }
}
