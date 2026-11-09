//! Everything one tick was given.

use energy_battery::protection::CellLatch;

use crate::dispatch::{FleetView, MeterReading, Overrides, Progress};
use crate::plan::Plan;
use crate::safety::Authority;

/// The inputs of one dispatcher tick.
///
/// `stale` is decided by the engine (replan trigger + plan coverage) and means "the plan may not be
/// executed this tick": the dispatcher then holds, because keeping the last setpoint is exactly the
/// failure mode the stale-input rule exists for. `latches` are the persisted cell-voltage states and
/// `overrides` are already merged (maintenance below operator).
pub struct TickInput<'a> {
    /// Tick instant, UTC epoch seconds.
    pub now: i64,
    /// Authority: `advisory`/`dry_run` write nothing.
    pub mode: Authority,
    /// The plan in force, when one is stored.
    pub plan: Option<&'a Plan>,
    /// The fleet read for this tick.
    pub fleet: &'a FleetView,
    /// The grid meter read for this tick.
    pub meter: &'a MeterReading,
    /// The cell-voltage latches in force.
    pub latches: &'a [CellLatch],
    /// The merged overrides in force.
    pub overrides: &'a Overrides,
    /// Whether the plan may not be executed (hold this tick).
    pub stale: bool,
    /// Heartbeat advanced after every write; `None` in tests.
    pub progress: Option<&'a dyn Progress>,
    /// The latched pack selection for this cycle, if one is persisted.
    pub selection: Option<&'a energy_ledger::selection::SelectionRow>,
}

impl std::fmt::Debug for TickInput<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TickInput")
            .field("now", &self.now)
            .field("mode", &self.mode)
            .field("stale", &self.stale)
            .field("packs", &self.fleet.packs.len())
            .finish_non_exhaustive()
    }
}
