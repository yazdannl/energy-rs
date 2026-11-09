//! The engine's configuration: everything an operator can change, in one place.

use energy_battery::protection::CellConfig;
use energy_battery::protection::full_charge::FullChargeSettings;

use crate::safety::Authority;

/// The engine's persisted policy.
///
/// Defaults are the shipped ones: `dry_run`, a 5 s tick, a fail-safe hold on error, a 15-minute
/// replan with a 30-minute stale bound, no grid caps (0 = unlimited) and protection off until the
/// operator configures it. `max_state_age_s` is the per-pack freshness bound; a reading older than
/// that is `stale`, which holds the pack instead of commanding it.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Control mode; persisted under `control.mode` and changed through the engine.
    pub mode: Authority,
    /// Tick interval, seconds.
    pub tick_seconds: u64,
    /// Whether a failed write is followed by one real hold.
    pub failsafe_on_error: bool,
    /// Normal replan interval, seconds.
    pub replan_seconds: i64,
    /// Absolute stale-plan bound, seconds.
    pub stale_plan_seconds: i64,
    /// Per-pack reserve floor above the pack minimum, percent.
    pub reserve_soc_pct: f64,
    /// Headroom kept free of grid energy at dispatch, percent of capacity.
    pub solar_reserve_pct: f64,
    /// Smallest allocation worth starting a pack for, W.
    pub min_pack_power_w: f64,
    /// Run as few packs as the request needs, best SoC first.
    pub pack_staggering: bool,
    /// SoC differences below this count as a tie in the pack order, percent.
    pub pack_switch_soc_pct: f64,
    /// How long a selected pack keeps its slot, seconds.
    pub pack_hold_seconds: i64,
    /// Grid deviation that forces a re-solve, W.
    pub grid_react_w: f64,
    /// PV deviation that forces a re-solve, W.
    pub solar_react_w: f64,
    /// Grid import ceiling, W (0 = unlimited).
    pub max_grid_import_w: f64,
    /// Grid export ceiling, W (0 = unlimited).
    pub max_grid_export_w: f64,
    /// Fleet charge ceiling before the per-pack split, W (0 = pack limits only).
    pub fleet_max_charge_w: f64,
    /// Fleet discharge ceiling before the per-pack split, W (0 = pack limits only).
    pub fleet_max_discharge_w: f64,
    /// A pack reading older than this is stale, seconds.
    pub max_state_age_s: f64,
    /// How long a pack is left alone after a control fault, seconds.
    pub fault_backoff_s: i64,
    /// How often completed sessions are re-extracted, seconds.
    pub session_check_s: i64,
    /// How far back a learning pass reads sessions, hours.
    pub session_window_hours: i64,
    /// Per-pack cell-voltage thresholds (the pack's own charge ceiling is added per pack).
    pub cell: CellConfig,
    /// Weekly full-charge maintenance.
    pub full_charge: FullChargeSettings,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            mode: Authority::DryRun,
            tick_seconds: 5,
            failsafe_on_error: true,
            replan_seconds: 900,
            stale_plan_seconds: 1800,
            reserve_soc_pct: 0.0,
            solar_reserve_pct: 0.0,
            min_pack_power_w: 50.0,
            pack_staggering: true,
            pack_switch_soc_pct: 2.0,
            pack_hold_seconds: 1800,
            grid_react_w: 10.0,
            solar_react_w: 10.0,
            max_grid_import_w: 0.0,
            max_grid_export_w: 0.0,
            fleet_max_charge_w: 0.0,
            fleet_max_discharge_w: 0.0,
            max_state_age_s: 180.0,
            fault_backoff_s: 10,
            session_check_s: 300,
            session_window_hours: 720,
            cell: CellConfig::default(),
            full_charge: FullChargeSettings::default(),
        }
    }
}

impl EngineConfig {
    /// The stale bound the replan trigger uses: `max(2 × replan, stale_plan_seconds)`.
    pub fn stale_after_s(&self) -> i64 {
        self.replan_seconds
            .saturating_mul(2)
            .max(self.stale_plan_seconds)
    }

    /// The watchdog's stall bound: three tick intervals.
    pub fn stall_after_s(&self) -> i64 {
        let tick = i64::try_from(self.tick_seconds).unwrap_or(i64::MAX);
        tick.saturating_mul(3)
    }
}
