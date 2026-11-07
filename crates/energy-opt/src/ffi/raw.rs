//! `eo_result`: pointers into the C result's arena.

use std::ffi::{c_long, c_void};
use std::ptr;

/// `eo_result` -- pointers into the C result's arena.
#[repr(C)]
#[derive(Debug)]
pub struct EoResultRaw {
    pub grid: *const f64,
    pub grid_len: c_long,
    pub value: *const f64,
    pub value_len: c_long,
    pub energies: *const f64,
    pub energies_len: c_long,
    pub powers: *const f64,
    pub powers_len: c_long,
    pub windows: *const f64,
    pub windows_len: c_long,
    pub effective_step_kwh: f64,
    pub objective_eur: f64,
    pub cycle_cost_eur: f64,
    pub terminal_error_kwh: f64,
    pub internal_objective_eur: f64,
    pub iterations: c_long,
    pub states: c_long,
    pub owner: *mut c_void,
}

impl Default for EoResultRaw {
    fn default() -> Self {
        Self {
            grid: ptr::null(),
            grid_len: 0,
            value: ptr::null(),
            value_len: 0,
            energies: ptr::null(),
            energies_len: 0,
            powers: ptr::null(),
            powers_len: 0,
            windows: ptr::null(),
            windows_len: 0,
            effective_step_kwh: 0.0,
            objective_eur: 0.0,
            cycle_cost_eur: 0.0,
            terminal_error_kwh: 0.0,
            internal_objective_eur: 0.0,
            iterations: 0,
            states: 0,
            owner: ptr::null_mut(),
        }
    }
}
