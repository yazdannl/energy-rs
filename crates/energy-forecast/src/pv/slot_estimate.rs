//! One slot's learned estimate, with the intermediate quantities the notes and status report.

/// One slot's learned estimate, with the intermediate quantities the notes and status report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotEstimate {
    /// Served AC power, kW.
    pub pv_kw: f64,
    /// The clamped model ratio.
    pub ratio: f64,
    /// The physics AC estimate before the ratio, W.
    pub physics_w: f64,
    /// Plane-of-array irradiance, W/m².
    pub poa_wm2: f64,
    /// Global horizontal irradiance handed to the model, W/m².
    pub ghi_wm2: f64,
    /// Clearness index.
    pub clearness_index: f64,
    /// NOCT cell temperature, degrees C.
    pub t_cell_c: f64,
    /// Sun elevation, degrees.
    pub elevation_deg: f64,
}

impl SlotEstimate {
    /// The zero estimate returned below the horizon without walking the model.
    pub(crate) fn night(elevation_deg: f64, t_ambient_c: f64) -> Self {
        Self {
            pv_kw: 0.0,
            ratio: 0.0,
            physics_w: 0.0,
            poa_wm2: 0.0,
            ghi_wm2: 0.0,
            clearness_index: 0.0,
            t_cell_c: t_ambient_c,
            elevation_deg,
        }
    }
}
