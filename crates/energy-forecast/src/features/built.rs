//! What the feature builder produced, with the physics the features describe.

use super::vector::Features;

/// The 28 features for one slot, plus the physical estimate they were built beside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuiltFeatures {
    /// The 28 features in contract order.
    pub features: Features,
    /// The AC estimate before the residual ratio, W.
    pub physics_w: f64,
    /// Plane-of-array irradiance, W/m².
    pub poa_wm2: f64,
}
