//! The engine value: construction, mode and overrides.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use energy_battery::protection::CellVoltageGuard;
use energy_battery::protection::full_charge::{FullChargeAction, FullChargeScheduler};
use energy_ledger::Ledger;

use crate::dispatch::{
    CommandSink, DeviceFleet, Dispatcher, ManualOverride, MeterSource, Overrides,
};
use crate::engine::config::EngineConfig;
use crate::engine::error::EngineError;
use crate::engine::instant;
use crate::engine::live_latch::LiveLatch;
use crate::engine::plan_source::PlanSource;
use crate::engine::state::EngineState;
use crate::engine::watchdog::Watchdog;
use crate::plan::Plan;
use crate::replan::ReplanTrigger;
use crate::safety::{Authority, RAMP_LIMIT_W, SafetyLimits};
use crate::wiring::latch_store::LedgerLatchStore;
use crate::wiring::learning_store::LedgerLearningStore;

/// The control engine.
///
/// One instance owns the plan, the live-write latch, the protection state and the override set, and
/// is the only thing that calls the [`CommandSink`]. The ports are trait objects so the engine can
/// be assembled from the real adapters in `energy-server` or from fakes in a test.
pub struct Engine {
    pub(crate) config: EngineConfig,
    pub(crate) fleet: Arc<dyn DeviceFleet>,
    pub(crate) meter: Arc<dyn MeterSource>,
    pub(crate) source: Arc<dyn PlanSource>,
    pub(crate) sink: Arc<dyn CommandSink>,
    pub(crate) ledger: Arc<Ledger>,
    pub(crate) dispatcher: Dispatcher,
    pub(crate) trigger: ReplanTrigger,
    pub(crate) cell_store: LedgerLatchStore,
    pub(crate) learning_store: LedgerLearningStore,
    pub(crate) state: Mutex<EngineState>,
}

impl Engine {
    /// Assemble an engine. Nothing is read or written until [`boot`](Engine::boot) runs.
    pub fn new(
        config: EngineConfig,
        fleet: Arc<dyn DeviceFleet>,
        meter: Arc<dyn MeterSource>,
        source: Arc<dyn PlanSource>,
        sink: Arc<dyn CommandSink>,
        ledger: Arc<Ledger>,
    ) -> Self {
        let now = Utc::now().timestamp();
        let dispatcher = Dispatcher::new(
            safety_limits(&config),
            config.failsafe_on_error,
            config.fault_backoff_s,
        );
        let trigger = ReplanTrigger {
            replan_seconds: config.replan_seconds,
            stale_plan_seconds: config.stale_plan_seconds,
            ..ReplanTrigger::default()
        };
        let state = EngineState {
            mode: config.mode,
            plan: None,
            latch: LiveLatch::new(),
            overrides: Vec::new(),
            watchdog: Watchdog::with_stall_after(config.stall_after_s()),
            cells: CellVoltageGuard::new(),
            maintenance: FullChargeScheduler::new(config.full_charge, instant(now)),
            maintenance_action: FullChargeAction::Idle,
            last_minute: None,
            last_session_check: None,
            pack_ids: std::collections::BTreeMap::new(),
            known_packs: Vec::new(),
            last_stale_log: None,
            last_refusal_log: None,
            latch_dirty: false,
        };
        Self {
            cell_store: LedgerLatchStore::new(Arc::clone(&ledger)),
            learning_store: LedgerLearningStore::new(Arc::clone(&ledger)),
            config,
            fleet,
            meter,
            source,
            sink,
            ledger,
            dispatcher,
            trigger,
            state: Mutex::new(state),
        }
    }

    /// The persisted policy.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// The engine's dispatcher (fault backoff state and operator bounds).
    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }

    /// The ledger this engine writes.
    pub fn ledger(&self) -> &Arc<Ledger> {
        &self.ledger
    }

    /// The control mode in force.
    pub fn mode(&self) -> Result<Authority, EngineError> {
        Ok(self.lock()?.mode)
    }

    /// The plan in force, if one is stored.
    pub fn current_plan(&self) -> Result<Option<Plan>, EngineError> {
        Ok(self.lock()?.plan.clone())
    }

    /// Change the control mode and persist it.
    ///
    /// Leaving `auto` neutralises what we left running first: a real hold for every latched pack,
    /// and nothing for hardware we never drove.
    pub async fn set_mode(&self, mode: Authority, now: i64) -> Result<(), EngineError> {
        if self.mode()? == mode {
            return Ok(());
        }
        if !mode.may_write() {
            self.release(now).await?;
        }
        crate::mode::save(&self.ledger, mode, now)?;
        self.lock()?.mode = mode;
        self.event(
            now,
            energy_ledger::EventLevel::Info,
            "mode.changed",
            format!("control mode is now {}", mode.slug()),
        )?;
        Ok(())
    }

    /// Add a per-pack manual override that expires after `ttl_s`.
    pub fn override_pack(
        &self,
        pack_id: impl Into<String>,
        action: crate::dispatch::OverrideAction,
        ttl_s: i64,
        now: i64,
    ) -> Result<(), EngineError> {
        let pack_id = pack_id.into();
        let until = now.saturating_add(ttl_s.max(1));
        let mut state = self.lock()?;
        state.overrides.retain(|entry| entry.active(now));
        state
            .overrides
            .push(ManualOverride::manual(pack_id.clone(), action, until));
        drop(state);
        self.event(
            now,
            energy_ledger::EventLevel::Info,
            "override.set",
            format!("{pack_id}: {} until {until}", action.slug()),
        )?;
        Ok(())
    }

    /// Drop every manual override.
    pub fn clear_overrides(&self) -> Result<(), EngineError> {
        self.lock()?.overrides.clear();
        Ok(())
    }

    /// The manual overrides in force at `now`.
    pub fn overrides(&self, now: i64) -> Result<Overrides, EngineError> {
        let state = self.lock()?;
        let entries: Vec<ManualOverride> = state
            .overrides
            .iter()
            .filter(|entry| entry.active(now))
            .cloned()
            .collect();
        Ok(Overrides::new(entries))
    }

    /// The live-write latch as persisted (for `/status` and the tests).
    pub fn latch(&self) -> Result<LiveLatch, EngineError> {
        Ok(self.lock()?.latch.clone())
    }

    /// Mark loop progress; the watchdog only ever moves forward.
    pub(crate) fn mark(&self, now: i64) -> Result<(), EngineError> {
        self.lock()?.watchdog.mark(now);
        Ok(())
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, EngineState>, EngineError> {
        self.state.lock().map_err(|_| EngineError::Poisoned)
    }
}

/// The operator's bounds, with the per-tick values (mode, fleet, floor) folded in later.
fn safety_limits(config: &EngineConfig) -> SafetyLimits {
    SafetyLimits {
        mode: config.mode,
        reserve_soc_pct: config.reserve_soc_pct,
        tick_seconds: config.tick_seconds as f64,
        ramp_limit_w: RAMP_LIMIT_W,
        max_grid_import_w: config.max_grid_import_w,
        max_grid_export_w: config.max_grid_export_w,
        fleet_max_charge_w: config.fleet_max_charge_w,
        fleet_max_discharge_w: config.fleet_max_discharge_w,
        ..SafetyLimits::default()
    }
}
