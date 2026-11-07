//! The solar source chain: kinds, interpolation, merge, capacity/derived physics, HA Energy series.

mod capacity;
mod capacity_slot;
mod derived;
mod ha_energy;
mod health;
mod kind;
mod merge;
mod merge_outcome;
mod merge_row;
mod series;
mod source;
mod state;

pub use capacity::{
    DEFAULT_UNCERTAINTY_PCT, UNCERTAINTY_CLEAR_PCT, UNCERTAINTY_CLOUD_PCT, UNCERTAINTY_NASA_PCT,
    attenuation_for, capacity_poa, capacity_slot, uncertainty_pct,
};
pub use capacity_slot::CapacitySlot;
pub use derived::{
    DERIVED_ENVELOPE_QUANTILE, DERIVED_MIN_MODEL_KW_PER_WP, DERIVED_MIN_SAMPLES, ImpliedCapacity,
    implied_capacity_wp,
};
pub use ha_energy::{HaEnergyPoints, ha_energy_power_series, parse_ha_energy_wh};
pub use health::SourceHealth;
pub use kind::SourceKind;
pub use merge::{MergeStrategy, merge_solar_series, merge_values};
pub use merge_outcome::MergeOutcome;
pub use merge_row::MergeRow;
pub use series::Series;
pub use source::SolarSource;
pub use state::SourceState;
