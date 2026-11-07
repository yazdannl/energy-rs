//! One pack's ladder answer, and the fleet split the ladder is applied to.

use energy_battery::protection::CellLatch;
use energy_ledger::plan::PlanSlot;

use crate::allocation::Allocation;
use crate::dispatch::pack_context::PackContext;
use crate::dispatch::{PackReading, TickInput, fleet_grid, grid_telemetry};
use crate::plan::PackState;
use crate::safety::{Authority, CommandRequest, Reason, SafetyDecision, SafetyLimits, check};

/// Split the target and bound the sum by the site's grid ceilings.
pub(crate) fn allocate(
    input: &TickInput<'_>,
    limits: &SafetyLimits,
    target: f64,
    slot: Option<&PlanSlot>,
) -> (Allocation, Option<Reason>) {
    let states: Vec<PackState> = input
        .fleet
        .packs
        .iter()
        .filter(|pack| pack.usable())
        .filter_map(PackReading::state)
        .collect();
    let blocked = crate::dispatch::pack_context::blocked_discharge(input.latches);
    let hours = limits.tick_seconds.max(1.0) / 3600.0;
    let mut allocation =
        Allocation::split(target, &states, limits.reserve_soc_pct, hours, &blocked);
    let base = grid_telemetry::base(input, slot, limits);
    let limit = fleet_grid::clamp(&mut allocation, base.as_ref(), limits);
    (allocation, limit)
}

/// The ladder's answer, or the fail-closed hold when the fleet grid check could not run.
pub(crate) fn decide(
    context: &PackContext<'_>,
    request: &CommandRequest,
    reading: &PackReading,
    latch: Option<&CellLatch>,
    shadow: bool,
) -> SafetyDecision {
    if context.grid_hold {
        return SafetyDecision::blocked(
            request.requested_w,
            0.0,
            0.0,
            vec![Reason::GridTelemetryMissing],
        );
    }
    if shadow {
        intended(context, request, reading, latch)
    } else {
        check(request, &reading.spec, context.limits, latch)
    }
}

/// The ladder's answer for `auto`; in a writing-disabled mode, the shadow of it.
fn intended(
    context: &PackContext<'_>,
    request: &CommandRequest,
    reading: &PackReading,
    latch: Option<&CellLatch>,
) -> SafetyDecision {
    if context.mode.may_write() {
        return check(request, &reading.spec, context.limits, latch);
    }
    let mut shadow = context.limits.clone();
    shadow.mode = Authority::Auto;
    check(request, &reading.spec, &shadow, latch)
}
