//! Energy dashboard discovery.
//!
//! Reads `energy/get_prefs` and maps the configured sources onto the internal model: grid import
//! and export, solar, home load, battery power and the optional `energy/solar_forecast` provider
//! output. Explicit per-entity overrides exist only to fill gaps.
//!
//! No code yet: P0 scaffold only.
