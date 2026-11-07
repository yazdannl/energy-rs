//! Safety clamp ladder.
//!
//! The ordered checks the old add-on applied (limits, SoC bounds, cell voltage, live-write latch,
//! stale data, grid/export constraints) with a machine-readable reason per clamp, so the UI can
//! explain exactly why a setpoint was reduced or blocked.
//!
//! No code yet: P0 scaffold only.
