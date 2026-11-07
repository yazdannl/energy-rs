//! Two solar physics chains, kept apart on purpose.
//!
//! * [`geometry`] / [`poa`] / [`power`] — the learned model's chain: UTC geometry, Erbs diffuse
//!   split, NOCT cell temperature, inverter efficiency and AC clipping.
//! * [`local_sun`] / [`sun_terms`] — the capacity/derived chain: local-clock geometry, constant 25%
//!   diffuse split, one performance ratio instead of a temperature model.

pub mod clear_sky;
pub mod constants;
pub mod geometry;
pub mod local_sun;
pub mod poa;
pub mod power;
pub mod site;
pub mod sun_terms;

pub use clear_sky::{
    clear_sky_ghi, cloud_attenuation, cloud_pct_from_attenuation, erbs_diffuse_fraction,
};
pub use constants::{
    CLOUD_COVERAGE_EXPONENT, CLOUD_MAX_ATTENUATION, DEFAULT_AIR_TEMPERATURE_C, DEFAULT_AZIMUTH_DEG,
    DEFAULT_PERFORMANCE_RATIO, DEFAULT_TILT_DEG, DIFFUSE_FRACTION, ETA_INV, G_STC, GAMMA_PER_C,
    GROUND_ALBEDO, HAURWITZ_A, HAURWITZ_B, MIN_COS_ZENITH, NOCT_C, SOLAR_CONSTANT, T_REF_C,
};
pub use geometry::{
    SolarGeometry, azimuth_deg, cos_aoi, declination_rad, elevation_deg, equation_of_time_min,
    hour_angle_rad, solar_geometry,
};
pub use local_sun::{CapacityConfig, plane_irradiance};
pub use poa::{PlaneOfArray, plane_of_array};
pub use power::{ac_power, cell_temperature, dc_power};
pub use site::Site;
pub use sun_terms::{LocalSunTerms, sun_terms};
