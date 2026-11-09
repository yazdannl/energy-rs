//! The operator-configured bounds the ladder applies.

use crate::safety::{Authority, GRID_FOLLOW_DEADBAND_W, GRID_FOLLOW_MIN_BUDGET_W, RAMP_LIMIT_W};

/// Every bound outside the pack's own physics.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyLimits {
    /// Authority mode; advisory and dry-run write nothing.
    pub mode: Authority,
    /// Per-pack reserve floor above `min_soc_pct`, percent.
    pub reserve_soc_pct: f64,
    /// Headroom kept free of grid energy, percent of capacity (0 disables the clause).
    pub solar_reserve_pct: f64,
    /// Smallest allocation worth starting a pack for, W.
    pub min_pack_power_w: f64,
    /// Run as few packs as the request needs, best SoC first (off = proportional spread).
    pub pack_staggering: bool,
    /// Tick duration, seconds.
    pub tick_seconds: f64,
    /// Per-pack ramp, W/tick.
    pub ramp_limit_w: f64,
    /// Grid import ceiling, W (0 = unlimited).
    pub max_grid_import_w: f64,
    /// Grid export ceiling, W (0 = unlimited).
    pub max_grid_export_w: f64,
    /// Fleet charge cap before the per-pack split, W (0 = pack limits only).
    pub fleet_max_charge_w: f64,
    /// Fleet discharge cap before the per-pack split, W (0 = pack limits only).
    pub fleet_max_discharge_w: f64,
    /// Summed configured fleet capacity, kWh; scales the plan's per-pack SoC target.
    pub fleet_capacity_kwh: f64,
    /// The plan's own sell floor, EUR/kWh; `None` disables the cost-basis clause.
    pub sell_floor_eur_per_kwh: Option<f64>,
    /// Grid-follow deadband, W.
    pub grid_follow_deadband_w: f64,
    /// Minimum grid-follow budget, W.
    pub grid_follow_min_budget_w: f64,
}

impl Default for SafetyLimits {
    fn default() -> Self {
        Self {
            mode: Authority::Auto,
            reserve_soc_pct: 0.0,
            solar_reserve_pct: 0.0,
            min_pack_power_w: 50.0,
            pack_staggering: true,
            tick_seconds: 30.0,
            ramp_limit_w: RAMP_LIMIT_W,
            max_grid_import_w: 0.0,
            max_grid_export_w: 0.0,
            fleet_max_charge_w: 0.0,
            fleet_max_discharge_w: 0.0,
            fleet_capacity_kwh: 0.0,
            sell_floor_eur_per_kwh: None,
            grid_follow_deadband_w: GRID_FOLLOW_DEADBAND_W,
            grid_follow_min_budget_w: GRID_FOLLOW_MIN_BUDGET_W,
        }
    }
}
