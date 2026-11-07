//! Everything one served slot needs.

use chrono::{DateTime, Utc};

use crate::physics::Site;

/// Everything one served slot needs.
#[derive(Debug, Clone)]
pub struct SlotInputs {
    /// The slot instant, UTC.
    pub moment: DateTime<Utc>,
    /// Position and plane orientation.
    pub site: Site,
    /// DC capacity, Wp.
    pub dc_capacity_wp: f64,
    /// AC capacity, W.
    pub ac_capacity_wp: f64,
    /// Global horizontal irradiance handed to the model, W/m².
    pub ghi_wm2: f64,
    /// Ambient air temperature, degrees C.
    pub t_ambient_c: f64,
    /// Cloud coverage, percent.
    pub cloud_pct: f64,
    /// Attenuation actually applied to `ghi_wm2`.
    pub attenuation: f64,
    /// Wind speed, m/s.
    pub wind_ms: f64,
    /// Relative humidity, percent.
    pub rh_pct: f64,
    /// Precipitation, mm.
    pub rain_mm: f64,
}

impl SlotInputs {
    /// Inputs for one slot with the documented weather defaults (clear sky, calm, dry).
    pub fn new(
        moment: DateTime<Utc>,
        site: Site,
        dc_capacity_wp: f64,
        ac_capacity_wp: f64,
        ghi_wm2: f64,
        t_ambient_c: f64,
    ) -> Self {
        Self {
            moment,
            site,
            dc_capacity_wp,
            ac_capacity_wp,
            ghi_wm2,
            t_ambient_c,
            cloud_pct: 0.0,
            attenuation: 1.0,
            wind_ms: 0.0,
            rh_pct: 0.0,
            rain_mm: 0.0,
        }
    }
}
