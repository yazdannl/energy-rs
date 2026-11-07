//! Physics constants, shared by both chains where the constant itself is shared.

/// Extraterrestrial irradiance on a normal surface, W/m².
pub const SOLAR_CONSTANT: f64 = 1367.0;
/// Ground albedo used by the isotropic transposition.
pub const GROUND_ALBEDO: f64 = 0.2;
/// Nominal operating cell temperature, degrees C.
pub const NOCT_C: f64 = 45.0;
/// Reference cell temperature, degrees C.
pub const T_REF_C: f64 = 25.0;
/// Temperature coefficient of DC power, per degree C.
pub const GAMMA_PER_C: f64 = -0.004;
/// Inverter efficiency.
pub const ETA_INV: f64 = 0.96;
/// Irradiance at standard test conditions, W/m².
pub const G_STC: f64 = 1000.0;
/// Kasten-Czeplak maximum attenuation at full cloud.
pub const CLOUD_MAX_ATTENUATION: f64 = 0.75;
/// Kasten-Czeplak cloud-coverage exponent.
pub const CLOUD_COVERAGE_EXPONENT: f64 = 3.4;
/// Haurwitz clear-sky scale, W/m².
pub const HAURWITZ_A: f64 = 1098.0;
/// Haurwitz clear-sky exponent.
pub const HAURWITZ_B: f64 = 0.059;
/// Constant diffuse fraction of the capacity chain (deliberately not Erbs).
pub const DIFFUSE_FRACTION: f64 = 0.25;
/// Default plane tilt when none is configured.
pub const DEFAULT_TILT_DEG: f64 = 35.0;
/// Default plane azimuth when none is configured.
pub const DEFAULT_AZIMUTH_DEG: f64 = 180.0;
/// Default inverter performance ratio of the capacity chain.
pub const DEFAULT_PERFORMANCE_RATIO: f64 = 0.85;
/// Default ambient air temperature when nothing measures it.
pub const DEFAULT_AIR_TEMPERATURE_C: f64 = 15.0;
/// Floor on the cosine of the zenith angle inside the transposition.
pub const MIN_COS_ZENITH: f64 = 1e-6;
