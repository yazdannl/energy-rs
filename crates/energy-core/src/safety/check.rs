//! The authority gate, the pack state gate and the ordered ladder.

use energy_battery::protection::CellLatch;

use crate::plan::PackSpec;
use crate::safety::CommandRequest;
use crate::safety::Reason;
use crate::safety::SafetyLimits;
use crate::safety::decision::SafetyDecision;
use crate::safety::ladder::Ladder;

/// Apply the ordered clamp ladder to one pack's allocated setpoint.
///
/// Order (survey §7): authority -> pack SoC -> plan tracking -> power -> grid -> per-pack ramp ->
/// cost-basis floor -> cell-voltage latch. `request.requested_w` is the pack's share after
/// allocation, which is what makes the ramp bound per pack. A `lo > hi` conflict blocks and names
/// every binding bound; it never guesses a compromise.
pub fn check(
    request: &CommandRequest,
    pack: &PackSpec,
    limits: &SafetyLimits,
    latch: Option<&CellLatch>,
) -> SafetyDecision {
    if !limits.mode.may_write() {
        return authority(request);
    }
    if !request.measured.available {
        return block(request, Reason::BatteryUnavailable);
    }
    if request.measured.stale {
        return block(request, Reason::StaleState);
    }
    let energy = energy_kwh(request, pack, limits);
    Ladder::new(request, pack, limits, energy, latch).run()
}

fn authority(request: &CommandRequest) -> SafetyDecision {
    let mut reasons = vec![Reason::ModeNotAuto];
    if request.exit_hold {
        reasons.push(Reason::ExitHoldSuppressed);
    }
    SafetyDecision::blocked(request.requested_w, 0.0, 0.0, reasons)
}

fn block(request: &CommandRequest, reason: Reason) -> SafetyDecision {
    SafetyDecision::blocked(request.requested_w, 0.0, 0.0, vec![reason])
}

/// Prefer measured energy, then SoC × capacity, then the pack floor.
///
/// The floor fallback is the safe direction (the old add-on's rule): an unknown pack may still
/// charge, and the SoC floor gives it no discharge headroom. It is not a zero-fill: zero would
/// pretend the pack is empty and allow a charge it cannot hold.
fn energy_kwh(request: &CommandRequest, pack: &PackSpec, limits: &SafetyLimits) -> f64 {
    if let Some(energy) = request.measured.energy_kwh
        && energy.is_finite()
        && energy >= 0.0
    {
        return energy;
    }
    request
        .measured
        .soc_pct
        .filter(|soc| soc.is_finite() && *soc >= 0.0)
        .map_or_else(
            || pack.e_min_kwh(limits.reserve_soc_pct),
            |soc| pack.energy_kwh(soc),
        )
}
