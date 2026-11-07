//! The capacity source: `solar.py` physics, Kasten-Czeplak attenuation, its uncertainty band.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::capacity_slot::CapacitySlot;
use crate::physics::{CapacityConfig, Site, cloud_attenuation, plane_irradiance};

/// Uncertainty when only astronomy is known (no cloud data).
pub const UNCERTAINTY_CLEAR_PCT: f64 = 35.0;
/// Uncertainty with hourly cloud coverage.
pub const UNCERTAINTY_CLOUD_PCT: f64 = 25.0;
/// Uncertainty with NASA's observed local clearness.
pub const UNCERTAINTY_NASA_PCT: f64 = 30.0;
/// The default uncertainty a bare source reports.
pub const DEFAULT_UNCERTAINTY_PCT: f64 = 25.0;

/// The uncertainty band for a source's inputs.
pub fn uncertainty_pct(has_cloud: bool, has_nasa: bool) -> f64 {
    if has_cloud {
        UNCERTAINTY_CLOUD_PCT
    } else if has_nasa {
        UNCERTAINTY_NASA_PCT
    } else {
        UNCERTAINTY_CLEAR_PCT
    }
}

/// The attenuation a capacity slot applies: cloud coverage first, NASA clearness second, clear sky
/// otherwise. They are alternatives, never multiplied together.
pub fn attenuation_for(cloud_pct: Option<f64>, nasa_clearness: Option<f64>) -> f64 {
    match (cloud_pct, nasa_clearness) {
        (Some(coverage), _) => cloud_attenuation(coverage),
        (None, Some(clearness)) => clearness,
        (None, None) => 1.0,
    }
}

/// One capacity-source slot from the local-clock physics chain.
pub fn capacity_slot(
    config: &CapacityConfig,
    site: &Site,
    moment: DateTime<Utc>,
    zone: Tz,
    cloud_pct: Option<f64>,
    nasa_clearness: Option<f64>,
) -> CapacitySlot {
    let attenuation = attenuation_for(cloud_pct, nasa_clearness);
    CapacitySlot {
        pv_kw: config.slot_kw(site, moment, zone, attenuation),
        attenuation,
        uncertainty_pct: uncertainty_pct(cloud_pct.is_some(), nasa_clearness.is_some()),
    }
}

/// Plane-of-array irradiance for the capacity chain, exposed for the derived-size fit.
pub fn capacity_poa(site: &Site, moment: DateTime<Utc>, zone: Tz) -> f64 {
    plane_irradiance(site, moment, zone)
}
