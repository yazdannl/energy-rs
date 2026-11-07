//! PV forecast: GBT inference and its validation.
//!
//! Inference must be bit-exact against the old implementation: same feature order, same comparison
//! (`<=` on raw bit patterns), same missing-left rule, same output clamp. Any deviation shows up
//! against `pv_golden.json` and fails the build.
//!
//! No code yet: P0 scaffold only.
