//! The configuration is unusable.

use thiserror::Error;

/// The configuration is unusable.  Mirrors the domain checks the CPython extension ran inside
/// `parse_config` (`_core.c:630-658`); the C core repeats them so a caller of the C ABI gets the
/// same refusal, but Rust reports them typed.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ConfigError {
    #[error("config value {field} is not finite")]
    NonFinite { field: &'static str },
    #[error("soc bounds do not produce a finite span ({e_min_kwh} .. {e_max_kwh})")]
    EnergyBounds { e_min_kwh: f64, e_max_kwh: f64 },
    #[error(
        "battery efficiencies must be positive (eta_charge {eta_charge}, eta_discharge {eta_discharge})"
    )]
    Efficiencies { eta_charge: f64, eta_discharge: f64 },
    #[error(
        "battery power limits must be non-negative (p_charge_kw {p_charge_kw}, p_discharge_kw {p_discharge_kw})"
    )]
    PowerLimits {
        p_charge_kw: f64,
        p_discharge_kw: f64,
    },
    #[error("slot_minutes must be non-negative, got {0}")]
    SlotMinutes(f64),
    #[error("power_steps {0} is outside the supported range 0..=1000000")]
    PowerSteps(i64),
    #[error("max_states {0} is outside the supported range 8..=1000000")]
    MaxStates(i64),
    #[error("max_iterations must be positive, got {0}")]
    MaxIterations(i64),
    #[error("{field} does not fit the C core's integer type")]
    CountTooLarge { field: &'static str },
}
