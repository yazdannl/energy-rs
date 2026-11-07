//! Building the 28 features, in `pv.feature_vector`'s expression order.

use chrono::{DateTime, Datelike, Timelike, Utc};

use super::built::BuiltFeatures;
use super::names::{FEATURE_COUNT, index};
use super::vector::Features;
use crate::physics::constants::ETA_INV;
use crate::physics::{SolarGeometry, ac_power, cell_temperature, dc_power, plane_of_array};

/// Everything one slot's feature vector depends on.
#[derive(Debug, Clone)]
pub struct FeatureInputs<'a> {
    /// UTC geometry for the slot, including clear-sky GHI.
    pub geometry: &'a SolarGeometry,
    /// Global horizontal irradiance handed to the model, W/m².
    pub ghi_wm2: f64,
    /// Ambient air temperature, degrees C.
    pub t_ambient_c: f64,
    /// DC capacity, Wp.
    pub dc_capacity_wp: f64,
    /// AC capacity, W.
    pub ac_capacity_wp: f64,
    /// The slot instant, used for the UTC hour and day-of-year terms.
    pub moment: DateTime<Utc>,
    /// Cloud coverage, percent.
    pub cloud_pct: f64,
    /// Attenuation that was actually applied to `ghi_wm2`.
    pub attenuation: f64,
    /// Wind speed, m/s.
    pub wind_ms: f64,
    /// Relative humidity, percent.
    pub rh_pct: f64,
    /// Precipitation, mm.
    pub rain_mm: f64,
}

impl<'a> FeatureInputs<'a> {
    /// Inputs for one slot with the documented weather defaults (clear sky, calm, dry).
    pub fn new(
        geometry: &'a SolarGeometry,
        ghi_wm2: f64,
        t_ambient_c: f64,
        dc_capacity_wp: f64,
        ac_capacity_wp: f64,
        moment: DateTime<Utc>,
    ) -> Self {
        Self {
            geometry,
            ghi_wm2,
            t_ambient_c,
            dc_capacity_wp,
            ac_capacity_wp,
            moment,
            cloud_pct: 0.0,
            attenuation: 1.0,
            wind_ms: 0.0,
            rh_pct: 0.0,
            rain_mm: 0.0,
        }
    }
}

/// Build the feature vector for one slot.
///
/// `clipping_headroom` uses the **unclamped** `dc_power * eta`, which can go negative at cold cell
/// temperatures, while `physics_w` floors DC at zero first. That asymmetry is served behaviour, not
/// an oversight.
pub fn build(inputs: &FeatureInputs<'_>) -> BuiltFeatures {
    let geometry = inputs.geometry;
    let array = plane_of_array(
        inputs.ghi_wm2,
        geometry.cos_zenith,
        geometry.cos_incidence,
        geometry.tilt_deg,
    );
    let t_cell = cell_temperature(inputs.t_ambient_c, array.poa);
    let physics_w = ac_power(
        inputs.dc_capacity_wp,
        inputs.ac_capacity_wp,
        array.poa,
        t_cell,
    );
    let unclipped = dc_power(inputs.dc_capacity_wp, array.poa, t_cell) * ETA_INV;
    let clipping_headroom = (unclipped / inputs.ac_capacity_wp.max(1e-9) - 1.0).max(0.0);
    let hour_of_day = f64::from(inputs.moment.hour());
    let day_of_year = f64::from(inputs.moment.ordinal());
    let tilt_rad = geometry.tilt_deg.to_radians();
    let azimuth_rad = geometry.sun_azimuth_deg.to_radians();
    let mut values = [0.0_f64; FEATURE_COUNT];
    values[index::ELEVATION_DEG] = geometry.elevation_deg;
    values[index::COS_ZENITH] = geometry.cos_zenith;
    values[index::COS_INCIDENCE] = geometry.cos_incidence;
    values[index::AZIMUTH_SIN] = azimuth_rad.sin();
    values[index::AZIMUTH_COS] = azimuth_rad.cos();
    values[index::TILT_DEG] = geometry.tilt_deg;
    values[index::TILT_SIN] = tilt_rad.sin();
    values[index::GHI_WM2] = inputs.ghi_wm2;
    values[index::POA_WM2] = array.poa;
    values[index::CLEARNESS_INDEX] = array.clearness_index;
    values[index::DIFFUSE_FRACTION] = array.diffuse_fraction;
    values[index::DNI_WM2] = array.dni;
    values[index::T_AMBIENT_C] = inputs.t_ambient_c;
    values[index::T_CELL_C] = t_cell;
    values[index::DC_CAPACITY_KWP] = inputs.dc_capacity_wp / 1000.0;
    values[index::AC_CAPACITY_KWP] = inputs.ac_capacity_wp / 1000.0;
    values[index::DC_AC_RATIO] = inputs.dc_capacity_wp / inputs.ac_capacity_wp.max(1e-9);
    values[index::CLIPPING_HEADROOM] = clipping_headroom;
    values[index::HOUR_SIN] = (2.0 * std::f64::consts::PI * hour_of_day / 24.0).sin();
    values[index::HOUR_COS] = (2.0 * std::f64::consts::PI * hour_of_day / 24.0).cos();
    values[index::DOY_SIN] = (2.0 * std::f64::consts::PI * day_of_year / 365.0).sin();
    values[index::DOY_COS] = (2.0 * std::f64::consts::PI * day_of_year / 365.0).cos();
    values[index::CLOUD_PCT] = inputs.cloud_pct;
    values[index::ATTENUATION] = inputs.attenuation;
    values[index::CLEAR_SKY_GHI_WM2] = geometry.clear_sky_ghi_wm2;
    values[index::WIND_MS] = inputs.wind_ms;
    values[index::RH_PCT] = inputs.rh_pct;
    values[index::RAIN_MM] = inputs.rain_mm;
    BuiltFeatures {
        features: Features::new(values),
        physics_w,
        poa_wm2: array.poa,
    }
}
