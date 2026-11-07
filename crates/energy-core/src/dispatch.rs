//! Dispatcher.
//!
//! Translates the plan's current-slot intent into per-device setpoints, tracks actual SoC against
//! the planned SoC, applies the cost-basis sell floor, and writes through `energy-battery` with
//! echo confirmation. Dry-run mode produces the identical decision and logs it as intended.
//!
//! No code yet: P0 scaffold only.
