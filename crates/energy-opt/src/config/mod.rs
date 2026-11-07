//! The solver configuration: the 20 required keys, the 3 optional ones, and the PV preference.
//!
//! This is the Rust face of the CPython extension's config dict (`_core.c:66-98`,
//! `parse_config` at `_core.c:586`).  Field names are the dict keys verbatim so a call site can be
//! compared with the old add-on line for line; the optional keys are `Option`, and their presence
//! *is* the `has_*` flag the C core reads.
//!
//! `Config`, `BatterySpec` and `PvChargePreference` each live in their own file; this module is
//! only the wiring.

mod battery_spec;
mod pv_charge_preference;
mod settings;

pub use battery_spec::BatterySpec;
pub use pv_charge_preference::PvChargePreference;
pub use settings::{Config, MAX_DIMENSION, MIN_STATES};
