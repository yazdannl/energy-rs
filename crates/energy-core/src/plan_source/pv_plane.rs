//! One plane of the array: its own capacity, tilt and azimuth.

/// One roof plane. An array is a set of these; the forecast sums them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PvPlane {
    /// Display name (`PV SE`), free text.
    pub name: String,
    /// DC capacity of this plane, kWp.
    pub kwp: f64,
    /// Plane tilt, degrees from horizontal.
    pub tilt_deg: f64,
    /// Compass azimuth, degrees (0 = north, 180 = south).
    pub azimuth_deg: f64,
}

impl PvPlane {
    /// A usable plane: positive capacity and finite angles.
    pub fn usable(&self) -> bool {
        self.kwp.is_finite()
            && self.kwp > 0.0
            && self.tilt_deg.is_finite()
            && self.azimuth_deg.is_finite()
            && (0.0..=90.0).contains(&self.tilt_deg)
    }
}
