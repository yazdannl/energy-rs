//! The accepted optimizer result; a refusal is returned instead when unsafe.

use crate::optimizer::{BindingBound, ObjectiveBreakdown, ScheduleSlot, TieBreak};

/// A deterministic, constraint-respecting schedule and its exact objective.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerResult {
    /// One row per input slot, in the same order.
    pub schedule: Vec<ScheduleSlot>,
    /// Minimised objective with every term.
    pub objective: ObjectiveBreakdown,
    /// Stored energy after slot `N-1`.
    pub terminal_kwh: f64,
    /// `min(max(soft, floor), ceiling)`; independent of horizon length.
    pub effective_target_kwh: f64,
    /// `cheapest_buy / eta_charge`, EUR per stored kWh.
    pub salvage_eur_per_stored_kwh: f64,
    /// Canonical six-key signature of the emitted path.
    pub tie_break: TieBreak,
    /// Clips applied while meeting the site ceiling, when any.
    pub binding_bounds: Vec<BindingBound>,
}
