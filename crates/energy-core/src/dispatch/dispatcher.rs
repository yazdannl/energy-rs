//! The tick: read, allocate, plan every pack, validate the vector, then command.

mod backoff;
mod charge;
mod plan;
mod recovery;
mod row;
mod write;

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::allocation::ChargeSkip;
use crate::dispatch::grid_telemetry;
use crate::dispatch::import_ceiling::{Guard, NO_PLAN_TARGET_W};
use crate::dispatch::sent::Sent;
use crate::dispatch::{
    CommandSink, FaultGuard, PackReport, Planned, TickFrame, TickInput, TickOutcome, allocate,
    fleet_clamp, slot_power,
};
use crate::safety::{Reason, SafetyLimits};

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
    /// Packs whose device reported a charge ceiling; released only past the hysteresis margin.
    charge_latch: Mutex<BTreeSet<String>>,
}

/// One end-to-end per-pack budget, shared with every direct sink caller. It covers apply, echo
/// and the fail-safe hold together and stays below the 120 s health-staleness threshold.
const PACK_SEND_BUDGET: Duration = energy_battery::control::PACK_IO_BUDGET;

/// Per-send context: the tick clock, bypass flag, and whether fail-safe recovery started.
#[derive(Clone, Copy)]
struct SendContext<'a> {
    now: i64,
    /// Nonzero command accepted by this attempt, preserved through failed recovery.
    accepted_intent_w: Option<f64>,
    safety_traffic: bool,
    recovery_started: &'a AtomicBool,
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
            charge_latch: Mutex::new(BTreeSet::new()),
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
        let skips = charge::skips(self, input);
        let limits = self.limits_for(input);
        let frame = self.frame(input, &limits, &skips);
        let mut plans: Vec<Planned> = input
            .fleet
            .packs
            .iter()
            .map(|reading| plan::plan(self, &frame, reading))
            .collect();
        let fleet_limit = fleet_clamp::apply(&mut plans, &frame);
        let mut outcome = TickOutcome::new(input.now, input.mode);
        outcome.stale_input = frame.stale;
        outcome.plan_power_w = frame.plan_power_w;
        outcome.target_w = frame.target_w;
        outcome.grid_limit = frame.grid_limit;
        outcome.fleet_clamp = fleet_limit;
        outcome.capability_shortfall_w = frame.allocation.capability_shortfall_w;
        outcome.capability_skipped = frame.allocation.skipped_packs.clone();
        // One unreachable device must not hold the other three hostage. Every pack is an
        // independent future; `join_all` keeps fleet order while their bounded writes overlap.
        // Each sink still bounds its own transport, and a non-hold write no longer awaits flow.
        let reports = futures_util::future::join_all(plans.into_iter().map(|plan| {
            let frame = &frame;
            async move {
                let budgeted_plan = plan.clone();
                let recovery_started = AtomicBool::new(false);
                let Ok(report) = tokio::time::timeout(
                    PACK_SEND_BUDGET,
                    self.send_plan(frame, plan, sink, &recovery_started),
                )
                .await
                else {
                    self.faults.arm(
                        &budgeted_plan.pack_id,
                        input.now,
                        "send exceeded the end-to-end pack budget",
                    );
                    // The caller-side timeout may cancel the sink before its own handler marks the
                    // controller, so the uncertainty marking must happen here too.
                    sink.mark_uncertain(&budgeted_plan.pack_id, "dispatch");
                    return row::finish(
                        self,
                        budgeted_plan,
                        row::deadline_sent(recovery_started.load(Ordering::Relaxed)),
                    );
                };
                report
            }
        }))
        .await;
        outcome.packs.extend(reports);
        outcome
    }

    /// The per-tick values every pack shares: slot, split, grid base and ceilings.
    fn frame<'a>(
        &self,
        input: &'a TickInput<'a>,
        limits: &'a SafetyLimits,
        skips: &'a [ChargeSkip],
    ) -> TickFrame<'a> {
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
        let (baseline, _) = allocate(input, limits, replacement_w, slot.as_ref(), skips);
        let ceiling =
            self.ceiling
                .apply(input.now, replacement_w, input.meter, input.fleet, limits);
        let (allocation, grid_limit) =
            allocate(input, limits, ceiling.target_w, slot.as_ref(), skips);
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
            skips: skips.to_vec(),
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

    /// One pack, sent: the validated command, its receipt and its report.
    async fn send_plan(
        &self,
        frame: &TickFrame<'_>,
        plan: Planned,
        sink: &dyn CommandSink,
        recovery_started: &AtomicBool,
    ) -> PackReport {
        let skipped = plan.skip.is_some();
        let sent = if !skipped && plan.send {
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
                    accepted_intent_w: (plan.command_w.abs() > crate::dispatch::HOLD_DEADBAND_W)
                        .then_some(plan.command_w),
                    safety_traffic,
                    recovery_started,
                },
            )
            .await
        } else if skipped {
            row::skipped(&plan)
        } else {
            Sent::none()
        };
        row::finish(self, plan, sent)
    }
}
