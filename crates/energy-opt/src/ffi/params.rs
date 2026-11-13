//! `eo_core_params`: the flat parameter block, field for field with the C struct.

use std::os::raw::c_long;

/// `eo_core_params` -- the flat parameter block, field for field with the C struct.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EoCoreParams {
    pub e_min_kwh: f64,
    pub e_max_kwh: f64,
    pub p_charge_kw: f64,
    pub p_discharge_kw: f64,
    pub eta_charge: f64,
    pub eta_discharge: f64,
    pub start_kwh: f64,
    pub e_target: f64,
    pub slot_minutes: f64,
    pub soc_step_kwh: f64,
    pub power_steps: c_long,
    pub max_states: c_long,
    pub max_iterations: c_long,
    pub max_import_kw: f64,
    pub max_export_kw: f64,
    pub min_spread_eur_per_kwh: f64,
    pub sell_floor_eur_per_kwh: f64,
    pub wear_eur_per_kwh: f64,
    pub salvage: f64,
    pub target_penalty: f64,
    pub has_target: i32,
    pub allow_grid_charge: i32,
    pub allow_export: i32,
    pub refuse_battery_export: i32,
    pub has_sell_floor: i32,
    pub pv_preference: i32,
    pub align_actions: i32,
}
