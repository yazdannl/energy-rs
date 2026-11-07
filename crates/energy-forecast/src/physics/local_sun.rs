//! The capacity source's own physics: local-clock geometry, constant 25% diffuse split.
//!
//! This is deliberately **not** the learned chain. `solar.py` derives the hour angle from the local
//! clock and the local day of year, and splits the diffuse part with a constant instead of Erbs.
//! Sharing one function between the two tiers is the single most damaging refactor available here:
//! measured at 52°N on a winter morning, the learned POA was 110 W/m² against the capacity chain's
//! 247 W/m².

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::constants::{
    DEFAULT_PERFORMANCE_RATIO, DIFFUSE_FRACTION, GROUND_ALBEDO, HAURWITZ_A, HAURWITZ_B,
    MIN_COS_ZENITH,
};
use super::geometry::cos_aoi;
use super::site::Site;
use super::sun_terms::{elevation_deg, sun_terms};

/// Plane-of-array irradiance, W/m², for one instant; zero below the horizon.
pub fn plane_irradiance(site: &Site, moment: DateTime<Utc>, zone: Tz) -> f64 {
    let latitude_rad = site.latitude_rad();
    let terms = sun_terms(site.longitude, moment, zone);
    let elevation = elevation_deg(latitude_rad, terms.declination, terms.hour_angle);
    if elevation <= 0.0 {
        return 0.0;
    }
    let cos_zenith = (90.0 - elevation).to_radians().cos();
    if cos_zenith <= MIN_COS_ZENITH {
        return 0.0;
    }
    let ghi = HAURWITZ_A * cos_zenith * (-HAURWITZ_B / cos_zenith).exp();
    if ghi <= 0.0 {
        return 0.0;
    }
    let cos_incidence = cos_aoi(
        latitude_rad,
        terms.declination,
        terms.hour_angle,
        site.tilt_rad(),
        site.azimuth_from_south_rad(),
    );
    let cos_tilt = site.tilt_rad().cos();
    let diffuse = ghi * DIFFUSE_FRACTION;
    let beam = (ghi - diffuse) * cos_incidence.max(0.0) / cos_zenith;
    let poa =
        beam + diffuse * (1.0 + cos_tilt) / 2.0 + ghi * GROUND_ALBEDO * (1.0 - cos_tilt) / 2.0;
    poa.max(0.0)
}

/// The capacity source's tuning: array size and one performance ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapacityConfig {
    /// Installed DC capacity, Wp.
    pub capacity_wp: f64,
    /// Inverter/soiling performance ratio.
    pub performance_ratio: f64,
}

impl CapacityConfig {
    /// A configuration with the shipped default performance ratio.
    pub fn new(capacity_wp: f64) -> Self {
        Self {
            capacity_wp,
            performance_ratio: DEFAULT_PERFORMANCE_RATIO,
        }
    }

    /// One slot's AC power in kW, from the capacity chain's physics and an attenuation factor.
    pub fn slot_kw(&self, site: &Site, moment: DateTime<Utc>, zone: Tz, attenuation: f64) -> f64 {
        let poa = plane_irradiance(site, moment, zone);
        let gross =
            (self.capacity_wp / 1000.0) * (poa * attenuation / 1000.0) * self.performance_ratio;
        gross.max(0.0)
    }
}
