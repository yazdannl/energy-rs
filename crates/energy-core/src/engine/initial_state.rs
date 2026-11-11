//! The engine's mutable state at construction.

use energy_battery::protection::CellVoltageGuard;
use energy_battery::protection::full_charge::{FullChargeAction, FullChargeScheduler};

use crate::engine::config::EngineConfig;
use crate::engine::instant;
use crate::engine::live_latch::LiveLatch;
use crate::engine::state::EngineState;
use crate::engine::watchdog::Watchdog;

/// The engine's mutable state at construction: nothing known yet, nothing owned.
pub(super) fn initial_state(config: &EngineConfig, now: i64) -> EngineState {
    let started = instant(now);
    EngineState {
        booted: false,
        latch_corrupt: None,
        plan: None,
        plan_id: None,
        latch: LiveLatch::new(),
        selection: None,
        overrides: Vec::new(),
        watchdog: Watchdog::with_stall_after(config.stall_after_s()),
        cells: CellVoltageGuard::new(),
        maintenance: FullChargeScheduler::new(config.full_charge, started),
        maintenance_action: FullChargeAction::Idle,
        maintenance_pack: None,
        last_minute: None,
        last_session_check: None,
        pack_ids: std::collections::BTreeMap::new(),
        known_packs: Vec::new(),
        last_stale_log: None,
        last_refusal_log: None,
        refusal: None,
        latch_dirty: false,
        last_pool_save: None,
    }
}
