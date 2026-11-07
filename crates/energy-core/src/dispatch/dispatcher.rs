//! The tick: read, allocate, clamp, command and report.

use energy_battery::protection::CellLatch;
use energy_ledger::plan::PlanSlot;

use crate::allocation::Allocation;
use crate::dispatch::pack_context::{PackContext, measured, reason_for, requested_watts};
use crate::dispatch::{
    CommandSink, FaultGuard, HOLD_DEADBAND_W, PackReading, PackReport, SinkError, TickInput,
    TickOutcome, fleet_grid, slot_power,
};
use crate::plan::PackState;
use crate::safety::Reason;
use crate::safety::{Authority, CommandRequest, SafetyDecision, SafetyLimits, check};

/// Turns a plan and a fleet read into per-pack commands.
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
        let slot = input.plan.and_then(|plan| plan.slot_at(input.now)).cloned();
        let stale = input.stale || slot.is_none();
        let plan_power = if stale {
            None
        } else {
            input.plan.and_then(|plan| slot_power(plan, input.now))
        };
        let limits = self.limits_for(input);
        let target = plan_power.unwrap_or(0.0);
        let (allocation, grid_limit) = allocate(input, &limits, target, slot.as_ref());
        let context = PackContext {
            now: input.now,
            mode: input.mode,
            stale,
            slot: slot.as_ref(),
            limits: &limits,
            meter: input.meter,
            fleet_power_w: input.fleet.power_w(),
            latches: input.latches,
            overrides: input.overrides,
            progress: input.progress,
        };
        let mut outcome = TickOutcome::new(input.now, input.mode);
        outcome.stale_input = stale;
        outcome.plan_power_w = plan_power;
        outcome.target_w = target;
        outcome.grid_limit = grid_limit;
        for reading in &input.fleet.packs {
            let allocated = if stale {
                0.0
            } else {
                allocation.watts_for(reading.pack_id()).unwrap_or(0.0)
            };
            let report = self.pack_tick(&context, reading, allocated, sink).await;
            outcome.packs.push(report);
        }
        outcome
    }

    /// The operator's bounds with this tick's mode, fleet and sell floor folded in.
    fn limits_for(&self, input: &TickInput<'_>) -> SafetyLimits {
        let mut limits = self.limits.clone();
        limits.mode = input.mode;
        limits.fleet_capacity_kwh = input.fleet.capacity_kwh();
        limits.sell_floor_eur_per_kwh = input.plan.and_then(|plan| plan.sell_floor);
        limits
    }

    /// One pack: request, ladder, send, report.
    async fn pack_tick(
        &self,
        context: &PackContext<'_>,
        reading: &PackReading,
        allocated_w: f64,
        sink: &dyn CommandSink,
    ) -> PackReport {
        let pack_id = reading.pack_id();
        let latch = context
            .latches
            .iter()
            .find(|latch| latch.pack_id == pack_id);
        let overridden = context.overrides.active(pack_id, context.now);
        let requested = requested_watts(context, overridden, allocated_w);
        let request = CommandRequest::new(
            pack_id,
            requested,
            context.slot.cloned(),
            context.now,
            measured(context, reading),
        );
        let decision = check(&request, &reading.spec, context.limits, latch);
        let intended = intended(context, &request, reading, latch);
        let backoff = self.faults.blocking(pack_id, context.now);
        let sent = if context.mode.may_write() && !backoff && reading.available {
            self.send(context, pack_id, decision.allowed_w, sink).await
        } else {
            Sent::none()
        };
        PackReport {
            pack_id: pack_id.to_owned(),
            ledger_id: reading.ledger_id,
            requested_w: requested,
            intended_w: intended.allowed_w,
            applied_w: sent.applied_w,
            reason: reason_for(context, &decision, overridden.is_some(), backoff),
            intended_reason: intended.primary(),
            written: sent.written,
            verified: sent.verified,
            fail_safe: sent.fail_safe,
            error: sent.error,
        }
    }

    /// Write one pack, with the one-shot fail-safe hold when it fails.
    async fn send(
        &self,
        context: &PackContext<'_>,
        pack_id: &str,
        watts: f64,
        sink: &dyn CommandSink,
    ) -> Sent {
        let result = if watts.abs() <= HOLD_DEADBAND_W {
            sink.hold(pack_id).await
        } else {
            sink.write(pack_id, watts).await
        };
        if let Some(progress) = context.progress {
            progress.mark(context.now);
        }
        match result {
            Ok(receipt) => Sent {
                written: true,
                verified: receipt.verified,
                applied_w: receipt.applied_w,
                fail_safe: false,
                error: None,
            },
            Err(error) => self.fail_safe(context, pack_id, &error, sink).await,
        }
    }

    /// One real hold after a fault, then back off (survey §7 trap 10).
    async fn fail_safe(
        &self,
        context: &PackContext<'_>,
        pack_id: &str,
        error: &SinkError,
        sink: &dyn CommandSink,
    ) -> Sent {
        self.faults.arm(pack_id, context.now);
        let cause = error.to_string();
        if !self.failsafe_on_error {
            return Sent {
                error: Some(cause),
                ..Sent::none()
            };
        }
        match sink.hold(pack_id).await {
            Ok(receipt) => Sent {
                written: true,
                verified: receipt.verified,
                applied_w: receipt.applied_w,
                fail_safe: true,
                error: Some(format!("{cause}; fail-safe hold")),
            },
            Err(hold_error) => Sent {
                error: Some(format!("{cause}; fail-safe hold failed: {hold_error}")),
                ..Sent::none()
            },
        }
    }
}

/// Split the target and bound the sum by the site's grid ceilings.
fn allocate(
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
    let base = slot
        .map(|slot| slot.load_kw - slot.pv_kw)
        .or_else(|| input.meter.base_kw());
    let limit = fleet_grid::clamp(&mut allocation, base, limits);
    (allocation, limit)
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
