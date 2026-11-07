//! `OwnedResult`: the exactly-once owner of a C result arena.

use std::ffi::c_long;

use super::{EoResultRaw, eo_free};

/// A solve's C-owned result: released exactly once, when this value drops.
#[derive(Debug)]
pub struct OwnedResult(pub(crate) EoResultRaw);

impl OwnedResult {
    pub(crate) fn new(raw: EoResultRaw) -> Self {
        Self(raw)
    }

    // The receiver is not read: `&self` is what ties the returned borrow to the arena, so the
    // slice cannot outlive the result (and `eo_free`).
    #[allow(clippy::unused_self)]
    fn slice(&self, pointer: *const f64, len: c_long) -> &[f64] {
        if len <= 0 || pointer.is_null() {
            return &[];
        }
        let len = usize::try_from(len).unwrap_or(0);
        // SAFETY: the C side guarantees `len` finite doubles at `pointer` for as long as the
        // result is alive, and the returned borrow cannot outlive `&self`.
        unsafe { std::slice::from_raw_parts(pointer, len) }
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
        unsafe { eo_free(&raw mut self.0) }
    }
}
