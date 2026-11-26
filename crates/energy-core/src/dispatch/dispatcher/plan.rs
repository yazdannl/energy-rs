//! Decide one pack's request, ladder answer and dispatch row without doing I/O.

use crate::allocation::ChargeSkip;
use crate::dispatch::pack_context::{
    PackContext, cell_safety, measured, reason_for, requested_watts, telemetry_hold,
};
use crate::dispatch::{
    ManualOverride, OverrideAction, OverrideSource, PackReading, Planned, TickFrame, decide,
};
use crate::safety::{CommandRequest, EPS_W, Reason, SafetyDecision};

use super::Dispatcher;

/// Everything the ladder and the row builder need for one pack.
struct Inputs<'a> {
    overridden: Option<&'a ManualOverride>,
    requested_w: f64,
    /// The device ceiling verdict, carried only when this tick asks the pack to charge.
    skip: Option<&'a ChargeSkip>,
    backoff_cause: Option<String>,
    backoff: bool,
    guarded: bool,
    decision: SafetyDecision,
    intended: SafetyDecision,
    safety: Option<Reason>,
}

/// One pack, decided: request, ladder, shadow answer. No I/O.
pub(super) fn plan(
    dispatcher: &Dispatcher,
    frame: &TickFrame<'_>,
    reading: &PackReading,
) -> Planned {
    let inputs = inputs(dispatcher, frame, reading);
    assemble(frame, reading, inputs)
}

/// The charge ceiling applies only to a request that actually charges the pack.
///
/// The request is resolved from the override, the allocation or a guarded hold before this verdict
/// is taken: an override can ask a charge-skipped pack to charge even when its allocation is zero
/// or the tick target is non-positive. A zero request falls back to the post-guard tick target, so
/// a stale or charge-skipped pack with no allocation still keeps the `ChargeAtCeiling` verdict on a
/// positive tick. An import-guarded reversal or a discharge override carries no charge skip.
fn charge_skip<'a>(
    frame: &'a TickFrame<'_>,
    pack_id: &str,
    requested_w: f64,
) -> Option<&'a ChargeSkip> {
    let direction_w = if requested_w.abs() > EPS_W {
        requested_w
    } else {
        frame.target_w
    };
    if direction_w <= 0.0 {
        return None;
    }
    frame.skips.iter().find(|skip| skip.pack_id == pack_id)
}

fn inputs<'a>(
    dispatcher: &'a Dispatcher,
    frame: &'a TickFrame<'a>,
    reading: &'a PackReading,
) -> Inputs<'a> {
    let context = frame.context();
    let pack_id = reading.pack_id();
    let latch = context
        .latches
        .iter()
        .find(|latch| latch.pack_id == pack_id);
    let overridden = context.overrides.active(pack_id, context.now);
    let before = frame.baseline.watts_for(pack_id).unwrap_or(0.0);
    let allocated = frame.allocation.watts_for(pack_id).unwrap_or(0.0);
    let guarded = frame.import_ceiling && allocated < before - EPS_W;
    let mut requested_w = requested_watts(&context, overridden, allocated, guarded);
    // Device charge authority outranks even an override, but only on the charge side: resolve the
    // effective request first, then cancel a charging one. A discharge request runs the ladder.
    // An explicit neutral override is not a charge request either: its hold must reach the device
    // and its row must keep the override reason, even when the tick target is positive.
    let neutral_override = overridden
        .is_some_and(|entry| matches!(entry.action, OverrideAction::Hold | OverrideAction::Off));
    let skip = if neutral_override {
        None
    } else {
        charge_skip(frame, pack_id, requested_w)
    };
    if skip.is_some() {
        requested_w = 0.0;
    }
    let maintenance_full = overridden.is_some_and(|entry| {
        entry.source == OverrideSource::Maintenance
            && matches!(entry.action, OverrideAction::Charge { .. })
    });
    let request = CommandRequest::new(
        pack_id,
        requested_w,
        context.slot.cloned(),
        context.now,
        measured(&context, reading),
    )
    .with_solar_reserve_forecast(frame.solar_reserve_armed);
    let request = if maintenance_full {
        request.as_maintenance_full_charge()
    } else {
        request
    };
    let decision = decide(&context, &request, reading, latch, false);
    let intended = decide(&context, &request, reading, latch, true);
    let backoff_cause = dispatcher.faults.blocker(pack_id, context.now);
    let backoff = backoff_cause.is_some();
    let safety = cell_safety(latch, decision.allowed_w, context.mode).or(telemetry_hold(&decision));
    Inputs {
        overridden,
        requested_w,
        skip,
        backoff_cause,
        backoff,
        guarded,
        decision,
        intended,
        safety,
    }
}

fn assemble(frame: &TickFrame<'_>, reading: &PackReading, input: Inputs<'_>) -> Planned {
    let context = frame.context();
    let skipped = input.skip.is_some();
    // Cell holds, import-ceiling corrections and armed solar-telemetry holds are safety traffic:
    // backoff never suppresses them, and their fault state still backs off later writes.
    let vector = !skipped
        && reading.available
        && (input.safety.is_some() || input.guarded || !input.backoff);
    Planned {
        pack_id: reading.pack_id().to_owned(),
        ledger_id: reading.ledger_id,
        measured_w: reading.power_w,
        measured_flow_w: (reading.available && !reading.stale).then_some(reading.power_w),
        measured_at: (reading.available && !reading.stale).then_some(reading.observed_at),
        requested_w: input.requested_w,
        command_w: command_w(&input, context.mode.may_write()),
        reason: reason(&context, &input),
        intended_reason: input.intended.primary(),
        note: note(&input),
        skip: input.skip.cloned(),
        backoff_cause: input.backoff_cause,
        vector,
        send: vector && context.mode.may_write(),
    }
}

fn note(input: &Inputs<'_>) -> Option<String> {
    if let Some(skip) = input.skip {
        return Some(skip.note.clone());
    }
    crate::dispatch::pack_context::note_for(input.overridden)
}

fn command_w(input: &Inputs<'_>, may_write: bool) -> f64 {
    // Only a charge-side ceiling verdict cancels the command; a ceiling-latched pack may still
    // serve an import-guarded discharge or a negative plan.
    if input.skip.is_some() {
        0.0
    } else if may_write {
        input.decision.allowed_w
    } else {
        input.intended.allowed_w
    }
}

fn reason(context: &PackContext<'_>, input: &Inputs<'_>) -> Reason {
    if let Some(skip) = input.skip {
        return skip.reason;
    }
    input.safety.unwrap_or_else(|| {
        reason_for(
            context,
            &input.decision,
            input.overridden,
            input.backoff,
            input.guarded,
        )
    })
}
