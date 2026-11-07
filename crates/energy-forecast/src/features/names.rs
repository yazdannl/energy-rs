//! The feature contract: names, count and the (frozen) index of each column.

/// Number of features the model consumes.
pub const FEATURE_COUNT: usize = 28;

/// The feature contract, in order. The artifact carries the same list; the loader refuses a
/// mismatch rather than feeding a model the wrong column.
pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "sun_elevation_deg",
    "cos_zenith",
    "cos_incidence",
    "sun_azimuth_sin",
    "sun_azimuth_cos",
    "tilt_deg",
    "tilt_sin",
    "ghi_wm2",
    "poa_wm2",
    "clearness_index",
    "diffuse_fraction",
    "dni_wm2",
    "t_ambient_c",
    "t_cell_c",
    "dc_capacity_kwp",
    "ac_capacity_kwp",
    "dc_ac_ratio",
    "clipping_headroom",
    "hour_sin",
    "hour_cos",
    "doy_sin",
    "doy_cos",
    "cloud_pct",
    "attenuation",
    "clear_sky_ghi_wm2",
    "wind_ms",
    "rh_pct",
    "rain_mm",
];

/// Frozen column indices, so the serving code never searches the name list at runtime.
pub mod index {
    /// Sun elevation, degrees.
    pub const ELEVATION_DEG: usize = 0;
    /// Cosine of the solar zenith angle.
    pub const COS_ZENITH: usize = 1;
    /// Cosine of the angle of incidence on the plane.
    pub const COS_INCIDENCE: usize = 2;
    /// Sine of the sun azimuth.
    pub const AZIMUTH_SIN: usize = 3;
    /// Cosine of the sun azimuth.
    pub const AZIMUTH_COS: usize = 4;
    /// Configured plane tilt, degrees.
    pub const TILT_DEG: usize = 5;
    /// Sine of the plane tilt.
    pub const TILT_SIN: usize = 6;
    /// Estimated global horizontal irradiance.
    pub const GHI_WM2: usize = 7;
    /// Plane-of-array irradiance.
    pub const POA_WM2: usize = 8;
    /// Clearness index `kt`.
    pub const CLEARNESS_INDEX: usize = 9;
    /// Erbs diffuse fraction.
    pub const DIFFUSE_FRACTION: usize = 10;
    /// Direct normal irradiance.
    pub const DNI_WM2: usize = 11;
    /// Ambient air temperature, degrees C.
    pub const T_AMBIENT_C: usize = 12;
    /// NOCT cell temperature, degrees C.
    pub const T_CELL_C: usize = 13;
    /// DC capacity in kWp.
    pub const DC_CAPACITY_KWP: usize = 14;
    /// AC capacity in kW.
    pub const AC_CAPACITY_KWP: usize = 15;
    /// DC/AC ratio.
    pub const DC_AC_RATIO: usize = 16;
    /// Clipping headroom from the unclamped DC estimate.
    pub const CLIPPING_HEADROOM: usize = 17;
    /// Sine of the UTC hour.
    pub const HOUR_SIN: usize = 18;
    /// Cosine of the UTC hour.
    pub const HOUR_COS: usize = 19;
    /// Sine of the UTC day of year.
    pub const DOY_SIN: usize = 20;
    /// Cosine of the UTC day of year.
    pub const DOY_COS: usize = 21;
    /// Cloud coverage, percent.
    pub const CLOUD_PCT: usize = 22;
    /// Applied attenuation factor.
    pub const ATTENUATION: usize = 23;
    /// Haurwitz clear-sky GHI before attenuation.
    pub const CLEAR_SKY_GHI_WM2: usize = 24;
    /// Wind speed, m/s.
    pub const WIND_MS: usize = 25;
    /// Relative humidity, percent.
    pub const RH_PCT: usize = 26;
    /// Precipitation, mm.
    pub const RAIN_MM: usize = 27;
}
