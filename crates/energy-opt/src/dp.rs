//! Safe wrapper around the vendored C DP solver.
//!
//! P1 adds the `extern "C"` declarations, a `#[repr(C)]` mirror of the solver config struct and
//! the array marshalling. The public Rust API keeps the C signature shape:
//! `solve(pv, load, buy, sell, minutes, config)` with an optional terminal-value row.
//!
//! No code yet: P0 scaffold only.
