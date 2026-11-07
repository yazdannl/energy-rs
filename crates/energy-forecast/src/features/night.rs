//! The night gate.

/// Minimum elevation the training corpus kept rows above. Documentation only: serving gates at
/// zero and extrapolates in `(0, 5]`, exactly as `pv.py` does.
pub const MIN_ELEVATION_DEG: f64 = 5.0;

/// Whether production is physically zero at this sun elevation.
///
/// This is the only serving gate: `elevation <= 0` returns zeros without building features or
/// walking the model. Gating at [`MIN_ELEVATION_DEG`] instead would zero 52.5% of the golden
/// vectors that are daytime low-sun cases.
pub fn is_night(elevation_deg: f64) -> bool {
    elevation_deg <= 0.0
}
