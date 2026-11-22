//! One plan: the solver's timeline plus the header the ledger and the UI read.

use energy_battery::protection::full_charge::{FullChargeAssignment, UnassignedPack};
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
    /// The break-even an exported kWh had to clear, EUR/kWh; `None` when the gate is inert.
    pub export_floor_eur_per_kwh: Option<f64>,
    /// The cost basis the gate was armed with, EUR/kWh.
    pub cost_basis_eur_per_kwh: Option<f64>,
    /// The soft peak guideline in force, kWh.
    pub peak_reserve_kwh: Option<f64>,
    /// Headroom kept free of grid energy, kWh.
    pub solar_reserve_kwh: f64,
    /// PV a ceiling forced out of the pack, kWh.
    pub clipped_kwh: f64,
    /// Battery energy the export gate kept in the pack, kWh.
    pub gated_kwh: f64,
    /// Money and energy totals.
    pub summary: PlanSummary,
    /// Why the forecasts were degraded (an EWMA/fallback series was planned with), `None` when the
    /// provider forecasts were fresh.
    pub degraded: Option<String>,
    /// The 15-minute timeline.
    pub slots: Vec<PlanSlot>,
    /// Price-aware maintenance charges folded into this plan, one per due pack.
    pub maintenance: Vec<FullChargeAssignment>,
    /// Due packs this plan could not schedule, with reasons and missing energy.
    pub maintenance_unassigned: Vec<UnassignedPack>,
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

    /// Whether any slot still ahead in the horizon forecasts PV surplus.
    ///
    /// This is the dispatch-time form of the planner's `solar_reserve` verdict: the reserve is
    /// released only when the whole remaining horizon is sunless. A stale or missing plan is not
    /// proof of that, so the caller leaves the reserve armed in that case.
    #[must_use]
    pub fn has_remaining_pv_surplus(&self, now: i64) -> bool {
        self.slots.iter().any(|slot| {
            slot.t_start.saturating_add(SLOT_SECONDS) > now && (slot.pv_kw - slot.load_kw) > 1e-9
        })
    }
}
