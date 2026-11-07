//! Forecasts.
//!
//! Responsibility (PLAN.md §2, §7): predict PV and load for the 120-slot horizon, in Rust, with no
//! Python at runtime.
//!
//! * [`pv`] — the bit-exact port of the shipped GBT (28 features, 400 trees, IEEE-754 bit patterns
//!   for thresholds/values, missing-left handling, clamp 0.3-1.8) plus [`physics`] and an EWMA
//!   fallback when the model cannot run.
//! * [`load`] — the 48-bucket load profile from HA long-term statistics, night-derived tier,
//!   named device subtraction and online EWMA update.
//! * [`artifacts`] — loading and integrity-checking the model artifacts (`pv_base.json`,
//!   `pv_golden.json`) at startup.
//!
//! Golden-vector equality is a hard gate for P4: 800/800 vectors, exact.

pub mod artifacts;
pub mod load;
pub mod physics;
pub mod pv;
