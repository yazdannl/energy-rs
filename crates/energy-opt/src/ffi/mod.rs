//! The raw C ABI (`csrc/core_solver.h`) and its safe owner.
//!
//! The C side owns every output buffer; [`EoResultRaw`] is released by `eo_free` exactly once,
//! which is why this module hands out an [`OwnedResult`] rather than a struct a caller could copy.

use std::ffi::{CStr, c_char, c_int};
use std::ptr;

mod owned;
mod params;
mod raw;

pub use owned::OwnedResult;
pub use params::EoCoreParams;
pub use raw::EoResultRaw;

/// The C ABI's slot ceiling (`EO_MAX_SLOTS`, which the C side static-asserts equals the core's).
pub const EO_MAX_SLOTS: usize = 1_000_000;

pub const EO_OK: c_int = 0;
pub const EO_ERR_ARGUMENT: c_int = -1;
pub const EO_ERR_LIMITS: c_int = -2;
pub const EO_ERR_DOMAIN: c_int = -3;
pub const EO_ERR_GRID: c_int = -4;
pub const EO_ERR_TERMINAL_ROW: c_int = -7;

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

/// One C solve, with the parameter block and arrays marshalled and the result owned.
pub(crate) fn solve(
    params: &EoCoreParams,
    input: &crate::dp::SolveInput<'_>,
    terminal_row: Option<&[f64]>,
) -> Result<OwnedResult, i32> {
    let mut raw = EoResultRaw::default();
    let row_pointer = terminal_row.map_or(ptr::null(), <[f64]>::as_ptr);
    let row_len = match terminal_row {
        Some(row) => i32::try_from(row.len()).unwrap_or(i32::MAX),
        None => 0,
    };
    // SAFETY: every pointer refers to a live slice for the call's duration; the C side only reads
    // them, copies what it keeps, and validates the lengths and domains before the sweep.
    let code = unsafe {
        eo_solve(
            ptr::from_ref(params),
            i32::try_from(input.len()).unwrap_or(i32::MAX),
            input.pv_kw.as_ptr(),
            input.load_kw.as_ptr(),
            input.buy.as_ptr(),
            input.sell.as_ptr(),
            input.minutes.as_ptr(),
            row_pointer,
            row_len,
            ptr::from_mut(&mut raw),
        )
    };
    if code != EO_OK {
        return Err(code);
    }
    Ok(OwnedResult::new(raw))
}
