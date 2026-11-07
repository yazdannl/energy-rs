//! The raw C ABI (`csrc/core_solver.h`) and its safe owner.
//!
//! The C side owns every output buffer; [`EoResultRaw`] is released by [`eo_free`] exactly once,
//! which is why this module hands out a [`OwnedResult`] rather than a struct a caller could copy.

use std::ffi::{CStr, c_char, c_int, c_long, c_void};
use std::ptr;

/// The C ABI's slot ceiling (`EO_MAX_SLOTS`, which the C side static-asserts equals the core's).
pub const EO_MAX_SLOTS: usize = 1_000_000;

pub const EO_OK: c_int = 0;
pub const EO_ERR_ARGUMENT: c_int = -1;
pub const EO_ERR_LIMITS: c_int = -2;
pub const EO_ERR_DOMAIN: c_int = -3;
pub const EO_ERR_GRID: c_int = -4;
pub const EO_ERR_MEMORY: c_int = -5;
pub const EO_ERR_SWEEP: c_int = -6;
pub const EO_ERR_TERMINAL_ROW: c_int = -7;

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
}

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

unsafe extern "C" {
    pub fn eo_solve(
        params: *const EoCoreParams,
        n_slots: i32,
        pv_kw: *const f64,
        load_kw: *const f64,
        buy: *const f64,
        sell: *const f64,
        minutes: *const f64,
        terminal_row: *const f64,
        terminal_row_len: i32,
        out: *mut EoResultRaw,
    ) -> c_int;
    pub fn eo_free(result: *mut EoResultRaw);
    pub fn eo_last_error() -> *const c_char;
    pub fn eo_build_sha() -> *const c_char;
}

/// The last C error message on this thread.
pub(crate) fn last_error() -> String {
    // SAFETY: `eo_last_error` always returns a pointer to a NUL-terminated thread-local buffer.
    let message = unsafe { CStr::from_ptr(eo_last_error()) };
    message.to_string_lossy().into_owned()
}

/// The digest the C artifact was compiled with (`""` when the define was omitted).
pub(crate) fn build_sha() -> String {
    // SAFETY: `eo_build_sha` returns a pointer to a static NUL-terminated string literal.
    let sha = unsafe { CStr::from_ptr(eo_build_sha()) };
    sha.to_string_lossy().into_owned()
}

/// A solve's C-owned result: released exactly once, when this value drops.
#[derive(Debug)]
pub struct OwnedResult(pub(crate) EoResultRaw);

impl OwnedResult {
    fn slice(&self, pointer: *const f64, len: c_long) -> &[f64] {
        if len <= 0 || pointer.is_null() {
            return &[];
        }
        // SAFETY: the C side guarantees `len` finite doubles at `pointer` for as long as the
        // result is alive, and the returned borrow cannot outlive `&self`.
        unsafe { std::slice::from_raw_parts(pointer, len as usize) }
    }

    pub fn grid(&self) -> &[f64] {
        self.slice(self.0.grid, self.0.grid_len)
    }
    pub fn value(&self) -> &[f64] {
        self.slice(self.0.value, self.0.value_len)
    }
    pub fn energies(&self) -> &[f64] {
        self.slice(self.0.energies, self.0.energies_len)
    }
    pub fn powers(&self) -> &[f64] {
        self.slice(self.0.powers, self.0.powers_len)
    }
    pub fn windows(&self) -> &[f64] {
        self.slice(self.0.windows, self.0.windows_len)
    }
}

impl Drop for OwnedResult {
    fn drop(&mut self) {
        // SAFETY: this is the single owner; `eo_free` zeroes the struct and is safe on a
        // zeroed/owned value.
        unsafe { eo_free(&mut self.0) }
    }
}

/// One C solve, with the parameter block and arrays marshalled and the result owned.
pub(crate) fn solve(
    params: &EoCoreParams,
    input: &crate::dp::SolveInput<'_>,
    terminal_row: Option<&[f64]>,
) -> Result<OwnedResult, i32> {
    let mut raw = EoResultRaw::default();
    let row_pointer = terminal_row.map_or(ptr::null(), |row| row.as_ptr());
    let row_len = match terminal_row {
        Some(row) => i32::try_from(row.len()).unwrap_or(i32::MAX),
        None => 0,
    };
    // SAFETY: every pointer refers to a live slice for the call's duration; the C side only reads
    // them, copies what it keeps, and validates the lengths and domains before the sweep.
    let code = unsafe {
        eo_solve(
            params,
            i32::try_from(input.len()).unwrap_or(i32::MAX),
            input.pv_kw.as_ptr(),
            input.load_kw.as_ptr(),
            input.buy.as_ptr(),
            input.sell.as_ptr(),
            input.minutes.as_ptr(),
            row_pointer,
            row_len,
            &mut raw,
        )
    };
    if code != EO_OK {
        return Err(code);
    }
    Ok(OwnedResult(raw))
}
