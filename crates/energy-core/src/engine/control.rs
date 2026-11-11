//! The engine value: construction, mode and overrides.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use energy_ledger::Ledger;

use crate::dispatch::{
    CommandSink, DeviceFleet, Dispatcher, ManualOverride, MeterSource, Overrides,
};
use crate::engine::config::EngineConfig;
use crate::engine::error::EngineError;
use crate::engine::guarded_sink::GuardedSink;
use crate::engine::live_latch::LiveLatch;
use crate::engine::live_write_journal::LiveWriteJournal;
use crate::engine::plan_source::PlanSource;
use crate::engine::state::EngineState;
use crate::engine::write_gate::WriteGate;
use crate::plan::Plan;
use crate::replan::ReplanTrigger;
use crate::safety::{Authority, RAMP_LIMIT_W, SafetyLimits};
use crate::stream::{RuntimeView, SnapshotPublisher};
use crate::wiring::latch_store::LedgerLatchStore;
use crate::wiring::learning_store::LedgerLearningStore;

/// The control engine.
///
/// One instance owns the plan, the live-write latch, the protection state and the override set, and
/// is the only thing that calls the [`CommandSink`]. The ports are trait objects so the engine can
/// be assembled from the real adapters in `energy-server` or from fakes in a test.
///
/// Every command -- a tick's setpoint and a hold alike -- leaves through a
/// [`GuardedSink`], so authority, the control generation and the durable write intent are enforced
/// in exactly one place. The mode itself lives in the [`WriteGate`], not in the state mutex, so a
/// decision cannot be made under one mode and written under another.
pub struct Engine {
    pub(crate) config: EngineConfig,
    pub(crate) fleet: Arc<dyn DeviceFleet>,
    pub(crate) meter: Arc<dyn MeterSource>,
    pub(crate) source: Arc<dyn PlanSource>,
    pub(crate) policy: Option<Arc<crate::plan_source::PolicyHandle>>,
    pub(crate) policy_source: Option<Arc<dyn crate::plan_source::PolicySource>>,
    pub(crate) pool: Option<Arc<dyn crate::savings::PoolStore>>,
    pub(crate) sink: Arc<dyn CommandSink>,
    pub(crate) ledger: Arc<Ledger>,
    pub(crate) dispatcher: Dispatcher,
    pub(crate) trigger: ReplanTrigger,
    pub(crate) cell_store: LedgerLatchStore,
    pub(crate) learning_store: LedgerLearningStore,
    pub(crate) publisher: Option<Arc<dyn SnapshotPublisher>>,
    pub(crate) runtime: Option<Arc<dyn RuntimeView>>,
    pub(crate) gate: Arc<WriteGate>,
    pub(crate) journal: Arc<dyn crate::engine::WriteIntent>,
    pub(crate) state: Arc<Mutex<EngineState>>,
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
            grid_react_w: config.grid_react_w,
            solar_react_w: config.solar_react_w,
            ..ReplanTrigger::default()
        };
        let state = crate::engine::initial_state::initial_state(&config, now);
        let state = Arc::new(Mutex::new(state));
        let gate = Arc::new(WriteGate::new(config.mode));
        let journal = Arc::new(LiveWriteJournal::new(
            Arc::clone(&state),
            Arc::clone(&ledger),
        ));
        Self {
            cell_store: LedgerLatchStore::new(Arc::clone(&ledger)),
            learning_store: LedgerLearningStore::new(Arc::clone(&ledger)),
            config,
            fleet,
            meter,
            source,
            policy: None,
            policy_source: None,
            pool: None,
            sink,
            ledger,
            dispatcher,
            trigger,
            publisher: None,
            runtime: None,
            gate,
            journal,
            state,
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
    ///
    /// Read from the write gate, which is the same word every hardware command is admitted against:
    /// there is no second copy of the mode that could disagree with it.
    pub fn mode(&self) -> Result<Authority, EngineError> {
        Ok(self.gate.mode())
    }

    /// The plan in force, if one is stored.
    pub fn current_plan(&self) -> Result<Option<Plan>, EngineError> {
        Ok(self.lock()?.plan.clone())
    }

    /// Change the control mode and persist it.
    ///
    /// Leaving `auto` neutralises what we left running *while still in `auto`*: the generation is
    /// fenced first (so a tick that is mid-flight can no longer write), then every latched pack is
    /// held, then the mode moves. Entering `auto` is refused while the persisted latch is unreadable
    /// ([`reconcile_latch`](Engine::reconcile_latch) is the operator's way out).
    pub async fn set_mode(&self, mode: Authority, now: i64) -> Result<(), EngineError> {
        if self.mode()? == mode {
            return Ok(());
        }
        if mode.may_write()
            && let Some(bytes) = self.lock()?.latch_corrupt
        {
            return Err(EngineError::LatchCorrupt { bytes });
        }
        if !mode.may_write() {
            self.gate.fence();
            self.release(now).await?;
        }
        crate::mode::save(&self.ledger, mode, now)?;
        self.gate.transition(mode);
        self.event(
            now,
            energy_ledger::EventLevel::Info,
            "mode.changed",
            format!("control mode is now {}", mode.slug()),
        )?;
        Ok(())
    }

    /// Accept the loss of an unreadable live-write latch and allow `auto` again.
    ///
    /// This is an explicit operator action, never automatic: the stored value could not be read, so
    /// the packs it named are unknown and persisting an empty latch gives up on neutralising them.
    pub fn reconcile_latch(&self, now: i64) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        LiveLatch::new().save(&self.ledger, now)?;
        state.latch = LiveLatch::new();
        state.latch_corrupt = None;
        drop(state);
        self.event(
            now,
            energy_ledger::EventLevel::Warn,
            "latch.reconciled",
            "operator accepted the loss of the unreadable live-write latch; auto control allowed"
                .to_owned(),
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

    /// A gated sink bound to one control generation: the only way a command reaches a device.
    pub(crate) fn guarded(&self, token: u64, now: i64) -> GuardedSink {
        GuardedSink::new(
            Arc::clone(&self.sink),
            Arc::clone(&self.gate),
            Arc::clone(&self.journal),
            token,
            now,
        )
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
        solar_reserve_pct: config.solar_reserve_pct,
        min_pack_power_w: config.min_pack_power_w,
        pack_staggering: config.pack_staggering,
        pack_switch_soc_pct: config.pack_switch_soc_pct,
        pack_hold_seconds: config.pack_hold_seconds,
        grid_follow_deadband_w: config.grid_follow_deadband_w,
        tick_seconds: config.tick_seconds as f64,
        ramp_limit_w: RAMP_LIMIT_W,
        max_grid_import_w: config.max_grid_import_w,
        max_grid_export_w: config.max_grid_export_w,
        fleet_max_charge_w: config.fleet_max_charge_w,
        fleet_max_discharge_w: config.fleet_max_discharge_w,
        ..SafetyLimits::default()
    }
}
