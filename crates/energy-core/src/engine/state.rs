//! The state a tick reads and writes.

use std::collections::BTreeMap;

use energy_battery::protection::full_charge::{FullChargeAction, FullChargeScheduler};
use energy_battery::protection::{CellLatch, CellVoltageGuard};

use crate::dispatch::ManualOverride;
use crate::engine::LiveLatch;
use crate::engine::Refusal;
use crate::engine::Watchdog;
use crate::plan::Plan;

/// Everything the engine remembers between ticks.
///
/// The control mode is *not* here: it lives in the engine's
/// [`WriteGate`](crate::engine::WriteGate), which is also the only thing the write path consults,
/// so the mode a decision is made under and the mode a write is admitted under cannot disagree.
pub(crate) struct EngineState {
    /// Whether `boot` already ran; the loop only boots an engine that has not.
    pub(crate) booted: bool,
    /// Set when the persisted latch could not be read: `auto` control is refused until reconciled;
    /// the size of the stored value is kept for the log.
    pub(crate) latch_corrupt: Option<usize>,
    pub(crate) plan: Option<Plan>,
    pub(crate) plan_id: Option<i64>,
    pub(crate) latch: LiveLatch,
    /// The latched pack selection, loaded at boot and refreshed after every tick.
    pub(crate) selection: Option<energy_ledger::selection::SelectionRow>,
    pub(crate) overrides: Vec<ManualOverride>,
    pub(crate) watchdog: Watchdog,
    pub(crate) cells: CellVoltageGuard,
    pub(crate) maintenance: FullChargeScheduler,
    pub(crate) maintenance_action: FullChargeAction,
    /// Set by a manual `full_charge` request: the maintenance overrides apply to this pack only.
    pub(crate) maintenance_pack: Option<String>,
    pub(crate) last_minute: Option<i64>,
    pub(crate) last_session_check: Option<i64>,
    pub(crate) pack_ids: BTreeMap<String, i64>,
    pub(crate) known_packs: Vec<(String, Option<i64>)>,
    pub(crate) last_stale_log: Option<i64>,
    pub(crate) last_refusal_log: Option<i64>,
    /// Why the last solve was refused; cleared by the next plan that is accepted.
    pub(crate) refusal: Option<Refusal>,
    pub(crate) latch_dirty: bool,
}

impl EngineState {
    /// The latches as the dispatcher reads them, in stable pack order.
    pub(crate) fn latches(&self) -> Vec<CellLatch> {
        self.cells.latches().cloned().collect()
    }
}
