//! Local-clock declination and hour angle: the capacity/derived chain's own geometry.

use chrono::{DateTime, Datelike, Offset, Timelike, Utc};
use chrono_tz::Tz;

/// Declination and hour angle in radians, derived from the **local** clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSunTerms {
    /// Cooper's declination, radians.
    pub declination: f64,
    /// Hour angle, radians.
    pub hour_angle: f64,
}

/// Equation of time, minutes, using the local day of year.
fn equation_of_time_min(day_of_year: u32) -> f64 {
    let day = f64::from(day_of_year);
    let gamma = 2.0 * std::f64::consts::PI * (day - 1.0) / 365.0;
    229.18e-6
        * (75.0 + 1868.0 * gamma.cos()
            - 32077.0 * gamma.sin()
            - 14615.0 * (2.0 * gamma).cos()
            - 40849.0 * (2.0 * gamma).sin())
}

/// `(declination, hour angle)` for one instant, from the local clock and local day of year.
pub fn sun_terms(longitude: f64, moment: DateTime<Utc>, zone: Tz) -> LocalSunTerms {
    let local = moment.with_timezone(&zone);
    let day_of_year = local.ordinal();
    let offset_minutes = f64::from(local.offset().fix().local_minus_utc()) / 60.0;
    let clock_minutes = f64::from(local.hour()) * 60.0
        + f64::from(local.minute())
        + f64::from(local.second()) / 60.0;
    let true_solar =
        clock_minutes + equation_of_time_min(day_of_year) + 4.0 * longitude - offset_minutes;
    LocalSunTerms {
        declination: (23.45
            * (360.0 * (284.0 + f64::from(day_of_year)) / 365.0)
                .to_radians()
                .sin())
        .to_radians(),
        hour_angle: (true_solar / 4.0 - 180.0).to_radians(),
    }
}

/// Sun elevation, degrees, from local-clock terms.
pub fn elevation_deg(latitude_rad: f64, declination: f64, hour_angle: f64) -> f64 {
    let sin_elevation = latitude_rad.sin() * declination.sin()
        + latitude_rad.cos() * declination.cos() * hour_angle.cos();
    sin_elevation.clamp(-1.0, 1.0).asin().to_degrees()
}
