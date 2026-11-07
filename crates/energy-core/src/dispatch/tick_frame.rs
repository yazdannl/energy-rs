//! One tick's shared inputs: the slot in force, the split, the grid base and the context.

use energy_battery::protection::CellLatch;
use energy_ledger::plan::PlanSlot;

use crate::allocation::Allocation;
use crate::dispatch::fleet_grid::GridBase;
use crate::dispatch::pack_context::PackContext;
use crate::dispatch::{MeterReading, Overrides, Progress};
use crate::safety::{Authority, Reason, SafetyLimits};

/// Everything the per-pack planning and the fleet validation of one tick share.
///
/// The plan slot is owned here rather than borrowed from the tick input: it is a clone taken once,
/// and the per-pack [`PackContext`] borrows it, so the whole tick works off one consistent slot even
/// if the input plan is replaced meanwhile.
pub(crate) struct TickFrame<'a> {
    pub(crate) now: i64,
    pub(crate) mode: Authority,
    pub(crate) meter: &'a MeterReading,
    pub(crate) fleet_power_w: f64,
    pub(crate) latches: &'a [CellLatch],
    pub(crate) overrides: &'a Overrides,
    pub(crate) progress: Option<&'a dyn Progress>,
    pub(crate) limits: &'a SafetyLimits,
    pub(crate) slot: Option<PlanSlot>,
    pub(crate) stale: bool,
    pub(crate) allocation: Allocation,
    pub(crate) grid_base: Option<GridBase>,
    pub(crate) grid_limit: Option<Reason>,
    pub(crate) plan_power_w: Option<f64>,
    pub(crate) target_w: f64,
    pub(crate) grid_hold: bool,
}

impl TickFrame<'_> {
    /// The per-pack context every ladder call and helper reads.
    pub(crate) fn context(&self) -> PackContext<'_> {
        PackContext {
            now: self.now,
            mode: self.mode,
            stale: self.stale,
            slot: self.slot.as_ref(),
            limits: self.limits,
            meter: self.meter,
            fleet_power_w: self.fleet_power_w,
            latches: self.latches,
            overrides: self.overrides,
            grid_hold: self.grid_hold,
        }
    }
}
