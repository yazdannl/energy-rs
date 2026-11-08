//! The plan's money and energy totals, stored verbatim in `plans.summary_json`.

use serde::{Deserialize, Serialize};

/// The plan summary the UI and the ledger read.
///
/// Every number is the plan's own (solver) path, never a measurement. `infeasible_reason` is set
/// only when the optimizer returned a plan that misses a configured terminal target; the timeline
/// is still stored so the miss is visible instead of silently dropped (survey §7 trap 13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSummary {
    /// Grid cost of the planned path, EUR.
    pub cost_estimate_eur: f64,
    /// Grid cost with no battery movement, EUR.
    pub baseline_cost_eur: f64,
    /// Baseline minus the plan, EUR.
    pub saved_eur: f64,
    /// AC energy charged, kWh.
    pub charged_kwh: f64,
    /// AC energy discharged, kWh.
    pub discharged_kwh: f64,
    /// Charged energy that came from the grid, kWh.
    pub grid_charge_kwh: f64,
    /// Charged energy that came from PV, kWh.
    pub pv_charge_kwh: f64,
    /// Equivalent full cycles over the horizon.
    pub cycles: f64,
    /// Terminal target used for this solve, kWh.
    pub terminal_target_kwh: Option<f64>,
    /// `terminal_target_unreachable` when the target was missed.
    pub infeasible_reason: Option<String>,
    /// The break-even an exported kWh had to clear, EUR/kWh; `None` when the gate is inert.
    #[serde(default)]
    pub export_floor_eur_per_kwh: Option<f64>,
    /// The cost basis the gate was armed with, EUR/kWh.
    #[serde(default)]
    pub cost_basis_eur_per_kwh: Option<f64>,
    /// The soft peak guideline in force, kWh.
    #[serde(default)]
    pub peak_reserve_kwh: Option<f64>,
    /// Headroom kept free of grid energy, kWh.
    #[serde(default)]
    pub solar_reserve_kwh: f64,
    /// PV a ceiling forced out of the pack, kWh.
    #[serde(default)]
    pub clipped_kwh: f64,
    /// Battery energy the export gate kept in the pack, kWh.
    #[serde(default)]
    pub gated_kwh: f64,
}

impl PlanSummary {
    /// Serialize for `plans.summary_json`; the numbers are finite by construction.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}
