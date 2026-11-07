//! One engine tick: read, replan, dispatch, record.

use energy_battery::protection::CellLatch;

use crate::dispatch::{FleetView, Overrides, Progress, TickInput, TickOutcome};
use crate::engine::Engine;
use crate::engine::error::EngineError;
use crate::engine::plan_source::plan_states;
use crate::plan::Plan;
use crate::replan::{LiveState, ReplanReason};
use crate::safety::Authority;

/// The per-tick view of the engine's own state.
pub(crate) struct Prepared {
    pub(crate) mode: Authority,
    pub(crate) plan: Option<Plan>,
    pub(crate) stale: bool,
    pub(crate) overrides: Overrides,
    pub(crate) latches: Vec<CellLatch>,
}

impl Engine {
    /// Read, replan, dispatch and record exactly one tick.
    pub async fn tick_once(&self, now: i64) -> Result<TickOutcome, EngineError> {
        self.mark(now)?;
        let mut view = self.fleet.view(now).await?;
        self.mark(now)?;
        let meter = self.meter.reading().await;
        self.mark(now)?;
        self.wire(now, &mut view)?;
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
        };
        let outcome = self.dispatcher.tick(&input, self.sink.as_ref()).await;
        self.mark(now)?;
        self.record(&outcome, now)?;
        Ok(outcome)
    }

    /// Ask the P8a trigger whether the plan must be replaced; returns "hold this tick".
    ///
    /// A stale/expired/missing plan is never executed: the tick holds and the fresh solve lands for
    /// the next one. Every other trigger (slot boundary, SoC deviation, refreshed inputs) replans
    /// first and then executes the new plan in the same tick.
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
        let stale = matches!(
            reason,
            ReplanReason::NoPlan | ReplanReason::StaleInput | ReplanReason::PlanExpired
        );
        if stale {
            self.log_stale(now, reason)?;
        }
        self.replan(now, fleet).await;
        Ok(stale)
    }

    /// Solve a new plan and store it; a refusal leaves the previous plan alone.
    async fn replan(&self, now: i64, fleet: &FleetView) -> Option<Plan> {
        let packs = plan_states(&fleet.packs);
        if packs.is_empty() {
            self.log_refusal(now, "no pack has a usable state of charge");
            return None;
        }
        match self.source.plan(now, packs).await {
            Ok(plan) => {
                self.mark(now).ok();
                if let Err(error) = self.store_plan(&plan) {
                    tracing::warn!(%error, "plan could not be persisted");
                }
                if let Ok(mut state) = self.lock() {
                    state.plan = Some(plan.clone());
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
        let mut entries =
            crate::wiring::maintenance::overrides(state.maintenance_action, fleet, now);
        entries.extend(state.overrides.iter().cloned());
        let stale = hold || state.plan.is_none();
        Ok(Prepared {
            mode: state.mode,
            plan: state.plan.clone(),
            stale,
            overrides: Overrides::new(entries),
            latches: state.latches(),
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
