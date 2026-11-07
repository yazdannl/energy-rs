//! One plan: the solver's timeline plus the header the ledger and the UI read.

use energy_ledger::plan::PlanSlot;

use crate::plan::{PlanSummary, SLOT_SECONDS};

/// A generated plan.
///
/// The timeline reuses [`energy_ledger::plan::PlanSlot`] so a plan can be inserted without a second
/// mapping layer. `sell_floor` is the plan's own cost-basis floor (the horizon's minimum marginal
/// export value); safety only clamps the plan's own planned discharges against it.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Plan instant, UTC epoch seconds.
    pub generated_at: i64,
    /// First slot start.
    pub horizon_start: i64,
    /// Exclusive end of the horizon.
    pub horizon_end: i64,
    /// Whether the terminal target (when set) was reached.
    pub feasible: bool,
    /// The solver's objective, EUR.
    pub objective_eur: f64,
    /// The sell floor this plan was solved with, EUR/kWh.
    pub sell_floor: Option<f64>,
    /// Money and energy totals.
    pub summary: PlanSummary,
    /// The 15-minute timeline.
    pub slots: Vec<PlanSlot>,
}

impl Plan {
    /// The slot containing `now`, if the horizon still covers it.
    pub fn slot_at(&self, now: i64) -> Option<&PlanSlot> {
        self.slots
            .iter()
            .find(|slot| slot.t_start <= now && now < slot.t_start.saturating_add(SLOT_SECONDS))
    }

    /// The plan's terminal target, when one was configured.
    pub fn terminal_target_kwh(&self) -> Option<f64> {
        self.summary.terminal_target_kwh
    }
}
