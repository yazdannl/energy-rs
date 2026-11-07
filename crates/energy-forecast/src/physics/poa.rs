//! Plane-of-array transposition with the Erbs split: the learned model's chain.

use super::clear_sky::erbs_diffuse_fraction;
use super::constants::{GROUND_ALBEDO, MIN_COS_ZENITH, SOLAR_CONSTANT};

/// The transposition result: what the model needs from one irradiance input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneOfArray {
    /// Plane-of-array irradiance, W/m², floored at zero.
    pub poa: f64,
    /// Clearness index, clamped to `[0, 1]`.
    pub clearness_index: f64,
    /// Erbs diffuse fraction.
    pub diffuse_fraction: f64,
    /// Direct normal irradiance, W/m².
    pub dni: f64,
}

/// `(poa, clearness index, diffuse fraction, dni)` from horizontal global irradiance, W/m².
///
/// The beam term is zeroed when the sun is behind the plane; the diffuse and ground-reflected
/// terms are not, which is why a north-facing array still produces something on a bright day.
pub fn plane_of_array(
    ghi: f64,
    cos_zenith: f64,
    cos_incidence: f64,
    tilt_deg: f64,
) -> PlaneOfArray {
    let cos_zenith = cos_zenith.max(MIN_COS_ZENITH);
    let kt = (ghi / (SOLAR_CONSTANT * cos_zenith)).clamp(0.0, 1.0);
    let diffuse_fraction = erbs_diffuse_fraction(kt);
    let dhi = diffuse_fraction * ghi;
    let dni = (ghi - dhi) / cos_zenith;
    let cos_tilt = tilt_deg.to_radians().cos();
    let poa = dni * cos_incidence.max(0.0)
        + dhi * (1.0 + cos_tilt) / 2.0
        + ghi * GROUND_ALBEDO * (1.0 - cos_tilt) / 2.0;
    PlaneOfArray {
        poa: poa.max(0.0),
        clearness_index: kt,
        diffuse_fraction,
        dni,
    }
}
