//! The array the PV model serves: plane, capacity and which forecast to use.

use super::PvMode;

/// The operator's PV array, from the settings.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PvConfig {
    /// DC capacity of the array, kWp.
    pub dc_kwp: f64,
    /// AC capacity of the inverter, kW.
    pub ac_kwp: f64,
    /// Plane tilt, degrees from horizontal.
    pub tilt_deg: f64,
    /// Compass azimuth, degrees (0 = north, 180 = south).
    pub azimuth_deg: f64,
    /// Which forecast drives the plan.
    pub mode: PvMode,
    /// A scale on the learned model's output, for calibrating it against the measured array.
    pub scale: f64,
}

impl PvConfig {
    /// Whether the learned model has everything it needs to run.
    pub fn model_ready(&self) -> bool {
        self.dc_kwp > 0.0 && self.ac_kwp > 0.0 && self.tilt_deg.is_finite()
    }

    /// The scale in force, never negative.
    pub fn scale(&self) -> f64 {
        if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }
}
