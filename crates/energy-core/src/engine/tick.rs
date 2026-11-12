//! One engine tick: read, replan, dispatch, record.

use energy_battery::protection::CellLatch;

use crate::dispatch::{FleetView, Overrides, Progress, TickInput, TickOutcome};
use crate::engine::Engine;
use crate::engine::error::EngineError;
use crate::engine::plan_source::plan_states;
use crate::plan::Plan;
use crate::replan::{LiveState, ReplanReason};
use crate::safety::Authority;
use energy_ledger::selection::SelectionRow;

/// The per-tick view of the engine's own state.
pub(crate) struct Prepared {
    pub(crate) mode: Authority,
    pub(crate) plan: Option<Plan>,
    pub(crate) stale: bool,
    pub(crate) overrides: Overrides,
    pub(crate) latches: Vec<CellLatch>,
    pub(crate) selection: Option<SelectionRow>,
}

impl Engine {
    /// Read, replan, dispatch and record exactly one tick.
    ///
    /// Progress is marked only after a step *succeeded*: a tick that fails in its first read never
    /// reaches a mark, so repeating failures still reach the watchdog's failure boundary instead of
    /// looking like work. The generation captured here fences every write of the tick: a hold or a
    /// mode change that lands mid-tick makes the remaining commands no-ops.
    pub async fn tick_once(&self, now: i64) -> Result<TickOutcome, EngineError> {
        match self.tick_inner(now).await {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                // The tick died before it could command anything: packs we drove earlier are held
                // now rather than left at their last setpoint until the next lucky tick.
                self.hold_orphans(now).await;
                Err(error)
            }
        }
    }

    /// One tick, without the orphan recovery the caller adds on failure.
    async fn tick_inner(&self, now: i64) -> Result<TickOutcome, EngineError> {
        let token = self.gate.token();
        let mut view = self.fleet.view(now).await?;
        self.mark(now)?;
        // The dashboard's battery power entity is optional; without it the site model cannot
        // balance the house, so the fleet's own measurement closes it here.
        let pack_w = (!view.packs.is_empty()).then(|| view.power_w());
        let meter = self.meter.reading().await.with_pack_power(pack_w);
        self.mark(now)?;
        self.refresh_policy(now, &view, &meter);
        self.wire(now, &mut view)?;
        self.mark(now)?;
        let hold = self.maybe_replan(now, &view).await?;
        let prepared = self.prepare(now, &view, hold)?;
        let input = TickInput {
            now,
            mode: prepared.mode,
            plan: prepared.plan.as_ref(),
            fleet: &view,
            meter: &meter,
            latches: &prepared.latches,
            overrides: &prepared.overrides,
            stale: prepared.stale,
            progress: Some(self),
            selection: prepared.selection.as_ref(),
        };
        let sink = self.guarded(token, now);
        let outcome = self.dispatcher.tick(&input, &sink).await;
        self.mark(now)?;
        self.record(&outcome, now)?;
        // Packs we own but could not command this tick are held before the tick is called done.
        self.reconcile(now, &outcome).await?;
        // And a pack that says it is doing something else is a control fault, not a transport one.
        self.check_readback(now, &outcome, &view)?;
        self.publish_tick(now, &view, &meter);
        Ok(outcome)
    }

    /// Ask the P8a trigger whether the plan must be replaced; returns "hold this tick".
    ///
    /// A stale/expired/missing plan is never executed: the tick holds and the fresh solve lands for
    /// the next one. Every other trigger (slot boundary, SoC deviation, refreshed inputs) replans
    /// first and then executes the new plan in the same tick. A **refused** solve always holds too:
    /// the previous plan is not a fallback, and the trigger fires again until a plan is accepted
    /// (`PLAN.md` §6).
    async fn maybe_replan(&self, now: i64, fleet: &FleetView) -> Result<bool, EngineError> {
        let plan = self.current_plan()?;
        let reason = self.trigger.reason(
            now,
            plan.as_ref(),
            &live_state(fleet),
            &self.source.changes(),
        );
        let Some(reason) = reason else {
            return Ok(false);
        };
        // A refusal holds; retrying it every tick would be a solve storm, so the next attempt waits.
        if self.refusal_holds_off(now)? {
            return Ok(true);
        }
        let stale = matches!(
            reason,
            ReplanReason::NoPlan | ReplanReason::StaleInput | ReplanReason::PlanExpired
        );
        if stale {
            self.log_stale(now, reason)?;
        }
        if self.replan(now, fleet).await.is_none() {
            // The refusal is journalled by `replan`; the hold is the safety half of it. Nothing is
            // dispatched against the previous plan while the source keeps refusing.
            self.log_stale(now, reason)?;
            return Ok(true);
        }
        Ok(stale)
    }

    /// Solve a new plan and store it; a refusal leaves the previous plan alone.
    pub(crate) async fn replan(&self, now: i64, fleet: &FleetView) -> Option<Plan> {
        let packs = plan_states(&fleet.packs);
        if packs.is_empty() {
            self.log_refusal(now, "no pack has a usable state of charge");
            return None;
        }
        match self.source.plan(now, packs).await {
            Ok(plan) => {
                self.mark(now).ok();
                let plan_id = match self.store_plan(&plan) {
                    Ok(id) => Some(id),
                    Err(error) => {
                        tracing::warn!(%error, "plan could not be persisted");
                        None
                    }
                };
                if let Ok(mut state) = self.lock() {
                    state.plan = Some(plan.clone());
                    state.plan_id = plan_id;
                    // An accepted plan clears the refusal fault: the hold is over.
                    state.refusal = None;
                }
                Some(plan)
            }
            Err(error) => {
                self.log_refusal(now, &error.to_string());
                None
            }
        }
    }

    /// Snapshot the state a tick needs, with overrides merged (maintenance below operator).
    fn prepare(&self, now: i64, fleet: &FleetView, hold: bool) -> Result<Prepared, EngineError> {
        let mut state = self.lock()?;
        state.overrides.retain(|entry| entry.active(now));
        let mut entries = crate::wiring::maintenance::overrides(
            state.maintenance_action,
            fleet,
            now,
            state.maintenance_pack.as_deref(),
        );
        entries.extend(state.overrides.iter().cloned());
        let stale = hold || state.plan.is_none();
        let mode = self.gate.mode();
        Ok(Prepared {
            mode,
            plan: state.plan.clone(),
            stale,
            overrides: Overrides::new(entries),
            latches: state.latches(),
            selection: state.selection.clone(),
        })
    }

    /// Persist what the tick measured and keep the derived state current.
    fn wire(&self, now: i64, view: &mut FleetView) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        crate::wiring::packs::resolve(&self.ledger, view, now, &mut state.pack_ids)?;
        state.known_packs = view
            .packs
            .iter()
            .map(|pack| (pack.pack_id().to_owned(), pack.ledger_id))
            .collect();
        crate::wiring::measurements::record(&self.ledger, now, view, &mut state.last_minute)?;
        let policy = crate::wiring::sessions::SessionPolicy::new(
            self.config.session_check_s,
            self.config.session_window_hours,
        );
        if crate::wiring::sessions::due(now, state.last_session_check, policy.check_seconds()) {
            let reports =
                crate::wiring::sessions::run(&self.ledger, &self.learning_store, now, &policy)?;
            for report in &reports {
                self.event(
                    now,
                    energy_ledger::EventLevel::Info,
                    "learning.applied",
                    report.summary(),
                )?;
            }
            state.last_session_check = Some(now);
        }
        let changed = crate::wiring::cells::evaluate(
            &self.cell_store,
            &mut state.cells,
            view,
            &self.config.cell,
            now,
        )?;
        for pack_id in changed {
            self.event(
                now,
                energy_ledger::EventLevel::Warn,
                "protection.latch",
                format!("{pack_id}: cell-voltage latch changed"),
            )?;
        }
        let action = crate::wiring::maintenance::consult(&mut state.maintenance, view, now);
        if action != state.maintenance_action && action.is_active() {
            self.event(
                now,
                energy_ledger::EventLevel::Info,
                "maintenance.action",
                format!("full-charge maintenance: {action:?}"),
            )?;
        }
        state.maintenance_action = action;
        if !action.is_active() {
            // A manual one-pack run is over; the next scheduled run covers the fleet again.
            state.maintenance_pack = None;
        }
        Ok(())
    }
}

/// The measured fleet energy and capacity the SoC-deviation trigger compares.
fn live_state(fleet: &FleetView) -> LiveState {
    let energy = fleet
        .packs
        .iter()
        .filter_map(|pack| {
            pack.energy_kwh.or_else(|| {
                pack.soc_pct
                    .filter(|soc| soc.is_finite())
                    .map(|soc| pack.spec.energy_kwh(soc))
            })
        })
        .sum();
    LiveState {
        energy_kwh: energy,
        capacity_kwh: fleet.capacity_kwh(),
    }
}

impl Progress for Engine {
    fn mark(&self, now: i64) {
        // A poisoned state mutex must not abort a tick; the supervisor will notice the stall.
        let _ = self.mark(now);
    }
}
