//! The array the PV model serves: its planes, the inverter and which forecast to use.

use super::{PvMode, PvPlane};

/// The operator's PV array, from the settings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PvConfig {
    /// The roof planes; their capacities sum to the array's DC size.
    pub planes: Vec<PvPlane>,
    /// AC capacity of the inverter, kW (one ceiling over all planes).
    pub ac_kwp: f64,
    /// Which forecast drives the plan.
    pub mode: PvMode,
    /// A scale on the learned model's output, for calibrating it against the measured array.
    pub scale: f64,
}

impl PvConfig {
    /// Whether the learned model has everything it needs to run.
    pub fn model_ready(&self) -> bool {
        !self.planes.is_empty() && self.ac_kwp > 0.0
    }

    /// The array's DC capacity, kWp.
    pub fn dc_kwp(&self) -> f64 {
        self.planes.iter().map(|plane| plane.kwp).sum()
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
