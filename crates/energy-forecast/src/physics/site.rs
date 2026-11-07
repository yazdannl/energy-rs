//! A site and its array: where the installation is and how the plane is oriented.

/// Latitude/longitude and plane orientation, passed to both physics chains.
///
/// The plane orientation belongs with the position because every geometry function needs both, and
/// because the two chains differ in how they derive the hour angle, not in what they are told about
/// the array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Site {
    /// Latitude, degrees.
    pub latitude: f64,
    /// Longitude, degrees.
    pub longitude: f64,
    /// Plane tilt, degrees.
    pub tilt_deg: f64,
    /// Compass azimuth, degrees (0 = north, 180 = south).
    pub azimuth_deg: f64,
}

impl Site {
    /// A site with the given position and plane orientation.
    pub fn new(latitude: f64, longitude: f64, tilt_deg: f64, azimuth_deg: f64) -> Self {
        Self {
            latitude,
            longitude,
            tilt_deg,
            azimuth_deg,
        }
    }

    /// Latitude in radians, which every geometry formula wants.
    pub fn latitude_rad(self) -> f64 {
        self.latitude.to_radians()
    }

    /// Plane tilt in radians.
    pub fn tilt_rad(self) -> f64 {
        self.tilt_deg.to_radians()
    }

    /// The plane azimuth in the transposition convention (0 = due south, positive toward west).
    pub fn azimuth_from_south_rad(self) -> f64 {
        (self.azimuth_deg + 180.0).rem_euclid(360.0).to_radians()
    }
}
