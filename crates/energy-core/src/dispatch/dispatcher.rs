//! The tick: read, allocate, plan every pack, validate the vector, then command.

mod backoff;
mod write;

use std::sync::Mutex;

use crate::dispatch::grid_telemetry;
use crate::dispatch::import_ceiling::{Guard, NO_PLAN_TARGET_W};
use crate::dispatch::pack_context::{
    cell_safety, measured, reason_for, requested_watts, telemetry_hold,
};
use crate::dispatch::sent::Sent;
use crate::dispatch::{
    AppliedPowerReason, CommandSink, FaultGuard, OverrideAction, OverrideSource, PackReading,
    PackReport, Planned, TickFrame, TickInput, TickOutcome, allocate, decide, fleet_clamp,
    slot_power,
};
use crate::safety::Reason;
use crate::safety::{CommandRequest, EPS_W, SafetyLimits};

/// Turns a plan and a fleet read into per-pack commands.
///
/// A tick has three phases: *plan* (pure: the split, the ladder, the netted-hold guard and the
/// shadow answer for every pack), *validate* (pure: the summed vector against the fleet power caps
/// and the site ceiling) and *send*. Nothing reaches a device before the whole vector has been
/// validated, so a later pack can never invalidate a bound an earlier one was checked against.
///
/// The dispatcher owns two small pieces of mutable state: the per-pack fault backoff and the
/// armed bit of the import-ceiling hysteresis. Everything else (authority, the plan, the latches,
/// the overrides) is handed in per tick, so the engine can change its mind between ticks without
/// remembering an old decision.
pub struct Dispatcher {
    limits: SafetyLimits,
    failsafe_on_error: bool,
    faults: FaultGuard,
    ceiling: Guard,
    /// Last non-hold command issued per pack, so a backoff row can name a real standing command.
    last_commands: Mutex<backoff::LastCommands>,
}

/// Per-send context: the tick clock and whether this command bypasses fault backoff.
#[derive(Clone, Copy)]
struct SendContext {
    now: i64,
    safety_traffic: bool,
}

impl Dispatcher {
    /// A dispatcher with the operator's bounds and the fault policy.
    pub fn new(limits: SafetyLimits, failsafe_on_error: bool, backoff_seconds: i64) -> Self {
        Self {
            limits,
            failsafe_on_error,
            faults: FaultGuard::new(backoff_seconds),
            ceiling: Guard::default(),
            last_commands: Mutex::new(backoff::LastCommands::new()),
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
        let frame = self.frame(input, &limits);
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
    fn frame<'a>(&self, input: &'a TickInput<'a>, limits: &'a SafetyLimits) -> TickFrame<'a> {
        let slot = input.plan.and_then(|plan| plan.slot_at(input.now)).cloned();
        let stale = input.stale || slot.is_none();
        let plan_power_w = if stale {
            None
        } else {
            input.plan.and_then(|plan| slot_power(plan, input.now))
        };
        // The plan's setpoint, corrected from the meter: the grid is what we actually hold.
        // With no executable plan the replacement is neutral: protection does not depend on an
        // economic plan being available, and a missing plan can never start a charge.
        let replacement_w = if stale {
            NO_PLAN_TARGET_W
        } else {
            crate::dispatch::follow_target::target(
                plan_power_w.unwrap_or(0.0),
                slot.as_ref(),
                input.meter,
                input.fleet,
                limits,
                stale,
            )
        };
        // Safety over the plan: a measured breach of the import ceiling lowers the setpoint only.
        // The pre-guard split is kept so the reason can be attributed to the packs it changed.
        let (baseline, _) = allocate(input, limits, replacement_w, slot.as_ref());
        let ceiling =
            self.ceiling
                .apply(input.now, replacement_w, input.meter, input.fleet, limits);
        let (allocation, grid_limit) = allocate(input, limits, ceiling.target_w, slot.as_ref());
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
            baseline,
            grid_base,
            grid_limit,
            plan_power_w,
            target_w: ceiling.target_w,
            import_ceiling: ceiling.active,
            // An absent plan cannot prove the horizon is sunless: keep the reserve armed.
            solar_reserve_armed: input
                .plan
                .is_none_or(|plan| plan.has_remaining_pv_surplus(input.now)),
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
        let before = frame.baseline.watts_for(pack_id).unwrap_or(0.0);
        let allocated = frame.allocation.watts_for(pack_id).unwrap_or(0.0);
        let guarded = frame.import_ceiling && allocated < before - EPS_W;
        // A stale plan normally holds every pack, but a protective guard discharge still goes out:
        // safety must not depend on an economic plan being fresh.
        let requested = requested_watts(&context, overridden, allocated, guarded);
        let maintenance_full = overridden.is_some_and(|entry| {
            entry.source == OverrideSource::Maintenance
                && matches!(entry.action, OverrideAction::Charge { .. })
        });
        let request = CommandRequest::new(
            pack_id,
            requested,
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
        let backoff_cause = self.faults.blocker(pack_id, context.now);
        let backoff = backoff_cause.is_some();
        // Cell holds, import-ceiling corrections and armed solar-telemetry holds are safety
        // traffic: backoff never suppresses them, and their fault state still backs off later writes.
        let safety =
            cell_safety(latch, decision.allowed_w, context.mode).or(telemetry_hold(&decision));
        let vector = reading.available && (safety.is_some() || guarded || !backoff);
        Planned {
            pack_id: pack_id.to_owned(),
            ledger_id: reading.ledger_id,
            measured_w: reading.power_w,
            measured_flow_w: (reading.available && !reading.stale).then_some(reading.power_w),
            requested_w: requested,
            command_w: if context.mode.may_write() {
                decision.allowed_w
            } else {
                intended.allowed_w
            },
            reason: safety
                .unwrap_or_else(|| reason_for(&context, &decision, overridden, backoff, guarded)),
            intended_reason: intended.primary(),
            note: crate::dispatch::pack_context::note_for(overridden),
            backoff_cause,
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
        let mut sent = if plan.send {
            // A verified cell-safety write is not proof the transport recovered: it bypasses the
            // backoff, so it must not clear the fault that is still suppressing ordinary writes.
            let safety_traffic = matches!(
                plan.reason,
                Reason::CellVoltageGuard
                    | Reason::CellVoltageEmergencyCharge
                    | Reason::GridImportCeiling
                    | Reason::SolarPvTelemetryMissing
                    | Reason::SolarLoadTelemetryMissing
            );
            self.send(
                frame.progress,
                &plan.pack_id,
                plan.command_w,
                sink,
                SendContext {
                    now: frame.now,
                    safety_traffic,
                },
            )
            .await
        } else {
            Sent::none()
        };
        backoff::finish(&self.last_commands, &plan, &mut sent);
        let applied_w_reason = if sent.flow_measured {
            None
        } else {
            Some(
                sent.applied_w_reason
                    .unwrap_or(AppliedPowerReason::SampleStale),
            )
        };
        PackReport {
            pack_id: plan.pack_id,
            ledger_id: plan.ledger_id,
            requested_w: plan.requested_w,
            intended_w: plan.command_w,
            applied_w: sent.applied_w,
            flow_measured: sent.flow_measured,
            applied_w_reason,
            reason: plan.reason,
            intended_reason: plan.intended_reason,
            written: sent.written,
            verified: sent.verified,
            fail_safe: sent.fail_safe,
            superseded: sent.superseded,
            error: sent.error,
            note: plan.note,
        }
    }
}
