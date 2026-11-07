//! Solar geometry for the Zonnebonus "daylight only" guard: geometric solar elevation from the
//! UTC instant and the site coordinates.  The approximation ignores the equation of time (±16 min),
//! which is well inside the guard's tolerance.

use chrono::{DateTime, Datelike, Timelike, Utc};

use crate::error::PriceError;

/// Site coordinates used to decide whether an instant is between sunrise and sunset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Daylight {
    latitude_deg: f64,
    longitude_deg: f64,
}

impl Daylight {
    /// Default location for this NL household (Amsterdam); configuration can override it.
    pub const fn netherlands() -> Self {
        Self {
            latitude_deg: 52.3676,
            longitude_deg: 4.9041,
        }
    }

    /// Validate and store a site location.
    pub fn new(latitude_deg: f64, longitude_deg: f64) -> Result<Self, PriceError> {
        if !latitude_deg.is_finite() || !(-90.0..=90.0).contains(&latitude_deg) {
            return Err(PriceError::Contract {
                field: "latitude_deg",
                value: latitude_deg,
            });
        }
        if !longitude_deg.is_finite() || !(-180.0..=180.0).contains(&longitude_deg) {
            return Err(PriceError::Contract {
                field: "longitude_deg",
                value: longitude_deg,
            });
        }
        Ok(Self {
            latitude_deg,
            longitude_deg,
        })
    }

    /// Latitude in degrees (positive north).
    pub const fn latitude_deg(&self) -> f64 {
        self.latitude_deg
    }

    /// Longitude in degrees (positive east).
    pub const fn longitude_deg(&self) -> f64 {
        self.longitude_deg
    }

    /// Whether the sun is above the horizon at `t_utc`.
    pub fn is_daylight(&self, t_utc: i64) -> bool {
        self.solar_elevation_deg(t_utc)
            .is_some_and(|elevation| elevation > 0.0)
    }

    /// Geometric solar elevation in degrees, or `None` for an unrepresentable instant.
    pub fn solar_elevation_deg(&self, t_utc: i64) -> Option<f64> {
        let utc = DateTime::<Utc>::from_timestamp(t_utc, 0)?;
        let day = f64::from(utc.ordinal());
        let declination_deg = 23.44 * (2.0 * std::f64::consts::PI * (day - 81.0) / 365.0).sin();
        let hours = f64::from(utc.hour()) + f64::from(utc.minute()) / 60.0;
        let solar_hours = hours + self.longitude_deg / 15.0;
        let hour_angle_deg = (solar_hours - 12.0) * 15.0;
        let latitude = self.latitude_deg.to_radians();
        let declination = declination_deg.to_radians();
        let hour_angle = hour_angle_deg.to_radians();
        let sin_elevation = latitude.sin() * declination.sin()
            + latitude.cos() * declination.cos() * hour_angle.cos();
        Some(sin_elevation.clamp(-1.0, 1.0).asin().to_degrees())
    }
}
