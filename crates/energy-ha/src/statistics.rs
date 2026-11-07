//! Long-term statistics access.
//!
//! Recorder statistics are read with `change` semantics for energy counters (`sum` snapshots are
//! resets/meter swaps), and normalized to Wh before they leave this module.
//!
//! No code yet: P0 scaffold only.
