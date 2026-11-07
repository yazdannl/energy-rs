//! The tick: read, allocate, plan every pack, validate the vector, then command.

use crate::dispatch::grid_telemetry;
use crate::dispatch::pack_context::{measured, reason_for, requested_watts};
use crate::dispatch::{
    CommandSink, FaultGuard, HOLD_DEADBAND_W, PackReading, PackReport, Planned, Progress,
    SinkError, TickFrame, TickInput, TickOutcome, advance, allocate, decide, fleet_clamp,
    slot_power,
};
use crate::safety::Reason;
use crate::safety::{CommandRequest, SafetyLimits};

/// Turns a plan and a fleet read into per-pack commands.
///
/// A tick has three phases: *plan* (pure: the split, the ladder and the shadow answer for every
/// pack), *validate* (pure: the summed vector against the fleet power caps and the site ceiling) and
/// *send*. Nothing reaches a device before the whole vector has been validated, so a later pack can
/// never invalidate the bound an earlier one was checked against.
///
/// The dispatcher owns exactly one piece of mutable state: the per-pack fault backoff. Everything
/// else (authority, the plan, the latches, the overrides) is handed in per tick, so the engine can
/// change its mind between ticks without the dispatcher remembering an old decision.
pub struct Dispatcher {
    limits: SafetyLimits,
    failsafe_on_error: bool,
    faults: FaultGuard,
}

/// What one send attempt produced.
struct Sent {
    written: bool,
    verified: bool,
    applied_w: f64,
    fail_safe: bool,
    error: Option<String>,
}

impl Sent {
    const fn none() -> Self {
        Self {
            written: false,
            verified: false,
            applied_w: 0.0,
            fail_safe: false,
            error: None,
        }
    }
}

impl Dispatcher {
    /// A dispatcher with the operator's bounds and the fault policy.
    pub fn new(limits: SafetyLimits, failsafe_on_error: bool, backoff_seconds: i64) -> Self {
        Self {
            limits,
            failsafe_on_error,
            faults: FaultGuard::new(backoff_seconds),
        }
    }

    /// The per-pack backoff state, for the engine's supervisor and the tests.
    pub fn faults(&self) -> &FaultGuard {
        &self.faults
    }

    /// The operator's bounds, before the per-tick mode and plan are folded in.
    pub fn limits(&self) -> &SafetyLimits {
        &self.limits
    }

    /// Run one tick.
    pub async fn tick(&self, input: &TickInput<'_>, sink: &dyn CommandSink) -> TickOutcome {
        self.faults.prune(input.now);
        let limits = self.limits_for(input);
        let frame = Self::frame(input, &limits);
        let mut plans: Vec<Planned> = input
            .fleet
            .packs
            .iter()
            .map(|reading| self.plan_pack(&frame, reading))
            .collect();
        let fleet_limit = fleet_clamp::apply(&mut plans, &frame);
        let mut outcome = TickOutcome::new(input.now, input.mode);
        outcome.stale_input = frame.stale;
        outcome.plan_power_w = frame.plan_power_w;
        outcome.target_w = frame.target_w;
        outcome.grid_limit = frame.grid_limit;
        outcome.fleet_clamp = fleet_limit;
        for plan in plans {
            outcome.packs.push(self.send_plan(&frame, plan, sink).await);
        }
        outcome
    }

    /// The per-tick values every pack shares: slot, split, grid base and ceilings.
    fn frame<'a>(input: &'a TickInput<'a>, limits: &'a SafetyLimits) -> TickFrame<'a> {
        let slot = input.plan.and_then(|plan| plan.slot_at(input.now)).cloned();
        let stale = input.stale || slot.is_none();
        let plan_power_w = if stale {
            None
        } else {
            input.plan.and_then(|plan| slot_power(plan, input.now))
        };
        let target_w = plan_power_w.unwrap_or(0.0);
        let (allocation, grid_limit) = allocate(input, limits, target_w, slot.as_ref());
        let grid_hold = grid_limit == Some(Reason::GridTelemetryMissing);
        let grid_base = grid_telemetry::base(input, slot.as_ref(), limits);
        TickFrame {
            now: input.now,
            mode: input.mode,
            meter: input.meter,
            fleet_power_w: input.fleet.power_w(),
            latches: input.latches,
            overrides: input.overrides,
            progress: input.progress,
            limits,
            slot,
            stale,
            allocation,
            grid_base,
            grid_limit,
            plan_power_w,
            target_w,
            grid_hold,
        }
    }

    /// The operator's bounds with this tick's mode, fleet and sell floor folded in.
    fn limits_for(&self, input: &TickInput<'_>) -> SafetyLimits {
        let mut limits = self.limits.clone();
        limits.mode = input.mode;
        limits.fleet_capacity_kwh = input.fleet.capacity_kwh();
        limits.sell_floor_eur_per_kwh = input.plan.and_then(|plan| plan.sell_floor);
        limits
    }

    /// One pack, decided: request, ladder, shadow answer. No I/O.
    fn plan_pack(&self, frame: &TickFrame<'_>, reading: &PackReading) -> Planned {
        let context = frame.context();
        let pack_id = reading.pack_id();
        let latch = context
            .latches
            .iter()
            .find(|latch| latch.pack_id == pack_id);
        let overridden = context.overrides.active(pack_id, context.now);
        let allocated = if context.stale {
            0.0
        } else {
            frame.allocation.watts_for(pack_id).unwrap_or(0.0)
        };
        let requested = requested_watts(&context, overridden, allocated);
        let request = CommandRequest::new(
            pack_id,
            requested,
            context.slot.cloned(),
            context.now,
            measured(&context, reading),
        );
        let decision = decide(&context, &request, reading, latch, false);
        let intended = decide(&context, &request, reading, latch, true);
        let backoff = self.faults.blocking(pack_id, context.now);
        let vector = reading.available && !backoff;
        Planned {
            pack_id: pack_id.to_owned(),
            ledger_id: reading.ledger_id,
            measured_w: reading.power_w,
            requested_w: requested,
            command_w: if context.mode.may_write() {
                decision.allowed_w
            } else {
                intended.allowed_w
            },
            reason: reason_for(&context, &decision, overridden, backoff),
            intended_reason: intended.primary(),
            note: crate::dispatch::pack_context::note_for(overridden),
            vector,
            send: vector && context.mode.may_write(),
        }
    }

    /// One pack, sent: the validated command, its receipt and its report.
    async fn send_plan(
        &self,
        frame: &TickFrame<'_>,
        plan: Planned,
        sink: &dyn CommandSink,
    ) -> PackReport {
        let sent = if plan.send {
            self.send(
                frame.progress,
                frame.now,
                &plan.pack_id,
                plan.command_w,
                sink,
            )
            .await
        } else {
            Sent::none()
        };
        PackReport {
            pack_id: plan.pack_id,
            ledger_id: plan.ledger_id,
            requested_w: plan.requested_w,
            intended_w: plan.command_w,
            applied_w: sent.applied_w,
            reason: plan.reason,
            intended_reason: plan.intended_reason,
            written: sent.written,
            verified: sent.verified,
            fail_safe: sent.fail_safe,
            error: sent.error,
            note: plan.note,
        }
    }

    /// Write one pack, with the one-shot fail-safe hold when it fails.
    async fn send(
        &self,
        progress: Option<&dyn Progress>,
        now: i64,
        pack_id: &str,
        watts: f64,
        sink: &dyn CommandSink,
    ) -> Sent {
        let result = if watts.abs() <= HOLD_DEADBAND_W {
            sink.hold(pack_id).await
        } else {
            sink.write(pack_id, watts).await
        };
        advance(progress, now);
        match result {
            Ok(receipt) => Sent {
                written: true,
                verified: receipt.verified,
                applied_w: receipt.applied_w,
                fail_safe: false,
                error: None,
            },
            // The gate withdrew the command (mode change, emergency hold) before it was written.
            // Nothing reached the device, so there is no fault to back off from either.
            Err(error @ SinkError::Fenced { .. }) => Sent {
                error: Some(error.to_string()),
                ..Sent::none()
            },
            Err(error) => self.fail_safe(progress, now, pack_id, &error, sink).await,
        }
    }

    /// One real hold after a fault, then back off (survey §7 trap 10).
    async fn fail_safe(
        &self,
        progress: Option<&dyn Progress>,
        now: i64,
        pack_id: &str,
        error: &SinkError,
        sink: &dyn CommandSink,
    ) -> Sent {
        self.faults.arm(pack_id, now);
        let cause = error.to_string();
        if !self.failsafe_on_error {
            return Sent {
                error: Some(cause),
                ..Sent::none()
            };
        }
        match sink.hold(pack_id).await {
            Ok(receipt) => {
                advance(progress, now);
                Sent {
                    written: true,
                    verified: receipt.verified,
                    applied_w: receipt.applied_w,
                    fail_safe: true,
                    error: Some(format!("{cause}; fail-safe hold")),
                }
            }
            Err(hold_error) => Sent {
                error: Some(format!("{cause}; fail-safe hold failed: {hold_error}")),
                ..Sent::none()
            },
        }
    }
}
