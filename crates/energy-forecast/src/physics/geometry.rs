//! UTC solar geometry: declination, equation of time, hour angle, elevation, azimuth, incidence.
//!
//! The day of year and the fractional hour are **UTC**, matching the training corpus. The capacity
//! chain's local-clock form lives in [`super::local_sun`]; the two are deliberately not unified.

use chrono::{DateTime, Datelike, Timelike, Utc};

use super::clear_sky::clear_sky_ghi;
use super::site::Site;

/// Sun and plane geometry for one instant, plus the clear-sky horizontal irradiance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolarGeometry {
    /// Sun elevation above the horizon, degrees.
    pub elevation_deg: f64,
    /// Cosine of the solar zenith angle.
    pub cos_zenith: f64,
    /// Cosine of the angle of incidence on the plane.
    pub cos_incidence: f64,
    /// Sun azimuth from north, clockwise (0 = north, 90 = east, 180 = south).
    pub sun_azimuth_deg: f64,
    /// Haurwitz clear-sky global horizontal irradiance, W/m².
    pub clear_sky_ghi_wm2: f64,
    /// The plane tilt the incidence was computed for, degrees.
    pub tilt_deg: f64,
}

/// Cooper's declination: ±23.45° at the solstices, zero at the equinoxes.
pub fn declination_rad(day_of_year: u32) -> f64 {
    (23.45
        * (360.0 * (284.0 + f64::from(day_of_year)) / 365.0)
            .to_radians()
            .sin())
    .to_radians()
}

/// Equation of time, minutes (fractional-year form; sub-hour detail is not modelled).
pub fn equation_of_time_min(day_of_year: u32) -> f64 {
    let day = f64::from(day_of_year);
    let gamma = 2.0 * std::f64::consts::PI * (day - 1.0) / 365.0;
    229.18e-6
        * (75.0 + 1868.0 * gamma.cos()
            - 32077.0 * gamma.sin()
            - 14615.0 * (2.0 * gamma).cos()
            - 40849.0 * (2.0 * gamma).sin())
}

/// Hour angle, radians, from a UTC fractional hour and the longitude.
pub fn hour_angle_rad(longitude: f64, day_of_year: u32, fractional_hour_utc: f64) -> f64 {
    let true_solar_minutes =
        fractional_hour_utc * 60.0 + equation_of_time_min(day_of_year) + 4.0 * longitude;
    (true_solar_minutes / 4.0 - 180.0).to_radians()
}

/// Sun elevation, degrees.
pub fn elevation_deg(latitude_rad: f64, declination: f64, hour_angle: f64) -> f64 {
    let sin_elevation = latitude_rad.sin() * declination.sin()
        + latitude_rad.cos() * declination.cos() * hour_angle.cos();
    sin_elevation.clamp(-1.0, 1.0).asin().to_degrees()
}

/// Sun azimuth from north, clockwise, degrees.
pub fn azimuth_deg(
    latitude_rad: f64,
    declination: f64,
    hour_angle: f64,
    elevation_rad: f64,
) -> f64 {
    let cos_azimuth = (declination.sin() - elevation_rad.sin() * latitude_rad.sin())
        / (elevation_rad.cos() * latitude_rad.cos());
    let azimuth = cos_azimuth.clamp(-1.0, 1.0).acos().to_degrees();
    if hour_angle > 0.0 {
        360.0 - azimuth
    } else {
        azimuth
    }
}

/// Cosine of the angle of incidence between the sun and a plane.
///
/// `azimuth_from_south_rad` follows the transposition formula's convention (0 = due south,
/// positive toward west); the caller converts the compass azimuth.
pub fn cos_aoi(
    latitude_rad: f64,
    declination: f64,
    hour_angle: f64,
    tilt_rad: f64,
    azimuth_from_south_rad: f64,
) -> f64 {
    declination.sin() * latitude_rad.sin() * tilt_rad.cos()
        - declination.sin() * latitude_rad.cos() * tilt_rad.sin() * azimuth_from_south_rad.cos()
        + declination.cos() * latitude_rad.cos() * tilt_rad.cos() * hour_angle.cos()
        + declination.cos()
            * latitude_rad.sin()
            * tilt_rad.sin()
            * azimuth_from_south_rad.cos()
            * hour_angle.cos()
        + declination.cos() * tilt_rad.sin() * azimuth_from_south_rad.sin() * hour_angle.sin()
}

/// Geometry for one UTC instant.
pub fn solar_geometry(site: &Site, moment: DateTime<Utc>) -> SolarGeometry {
    let day_of_year = moment.ordinal();
    let fractional_hour = f64::from(moment.hour())
        + f64::from(moment.minute()) / 60.0
        + f64::from(moment.second()) / 3600.0;
    let latitude_rad = site.latitude_rad();
    let declination = declination_rad(day_of_year);
    let hour_angle = hour_angle_rad(site.longitude, day_of_year, fractional_hour);
    let elevation = elevation_deg(latitude_rad, declination, hour_angle);
    let elevation_rad = elevation.to_radians();
    let cos_zenith = (90.0 - elevation).to_radians().cos();
    let incidence = cos_aoi(
        latitude_rad,
        declination,
        hour_angle,
        site.tilt_rad(),
        site.azimuth_from_south_rad(),
    );
    let azimuth = azimuth_deg(latitude_rad, declination, hour_angle, elevation_rad);
    SolarGeometry {
        elevation_deg: elevation,
        cos_zenith,
        cos_incidence: incidence,
        sun_azimuth_deg: azimuth,
        clear_sky_ghi_wm2: clear_sky_ghi(cos_zenith),
        tilt_deg: site.tilt_deg,
    }
}
