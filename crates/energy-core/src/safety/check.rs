//! The authority gate, the pack state gate and the ordered ladder.

use energy_battery::protection::CellLatch;

use crate::plan::PackSpec;
use crate::safety::CommandRequest;
use crate::safety::EPS_W;
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
    let Some(energy) = energy_kwh(request, pack) else {
        return unknown_state(request);
    };
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

/// A pack with no trustworthy state gets no setpoint.
///
/// Neither a charge nor a discharge can be justified when both the stored energy and the SoC are
/// unknown: charging may overfill a full pack and discharging may empty one. The old fallback to the
/// pack floor invented charge headroom, so it is gone; only the neutral hold is allowed, and the row
/// says `state_missing` so the operator sees why.
fn unknown_state(request: &CommandRequest) -> SafetyDecision {
    if request.requested_w.abs() <= EPS_W {
        return SafetyDecision::allowed(
            request.requested_w,
            0.0,
            0.0,
            0.0,
            vec![Reason::StateMissing],
        );
    }
    block(request, Reason::StateMissing)
}

/// Prefer measured energy, then SoC × capacity; `None` when neither is trustworthy.
fn energy_kwh(request: &CommandRequest, pack: &PackSpec) -> Option<f64> {
    if let Some(energy) = request.measured.energy_kwh
        && energy.is_finite()
        && energy >= 0.0
    {
        return Some(energy);
    }
    request
        .measured
        .soc_pct
        .filter(|soc| soc.is_finite() && *soc >= 0.0)
        .map(|soc| pack.energy_kwh(soc))
}
