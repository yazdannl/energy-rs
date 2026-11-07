//! Haurwitz clear sky, the Erbs diffuse split and Kasten-Czeplak cloud attenuation.

use super::constants::{CLOUD_COVERAGE_EXPONENT, CLOUD_MAX_ATTENUATION, HAURWITZ_A, HAURWITZ_B};

/// Haurwitz clear-sky horizontal global irradiance, W/m².
pub fn clear_sky_ghi(cos_zenith: f64) -> f64 {
    if cos_zenith <= 0.0 {
        return 0.0;
    }
    HAURWITZ_A * cos_zenith * (-HAURWITZ_B / cos_zenith).exp()
}

/// Erbs correlation: diffuse fraction of global horizontal irradiance from the clearness index.
pub fn erbs_diffuse_fraction(kt: f64) -> f64 {
    let kt = kt.clamp(0.0, 1.0);
    if kt <= 0.22 {
        return 1.0 - 0.09 * kt;
    }
    if kt <= 0.80 {
        // Python's `**` is libm `pow`, not repeated multiplication: `kt**2` and `kt * kt` can
        // differ by one ULP, which is enough to flip a split threshold downstream.
        return 0.9511 - 0.1604 * kt + 4.388 * kt.powf(2.0) - 16.638 * kt.powf(3.0)
            + 12.336 * kt.powf(4.0);
    }
    0.165
}

/// Kasten-Czeplak attenuation for a cloud coverage percentage.
///
/// Non-finite or absent coverage is treated as clear; that is the safe direction, because an
/// invented attenuation would silently cut a forecast nothing measured.
pub fn cloud_attenuation(cloud_pct: f64) -> f64 {
    if !cloud_pct.is_finite() {
        return 1.0;
    }
    let coverage = cloud_pct.clamp(0.0, 100.0) / 100.0;
    1.0 - CLOUD_MAX_ATTENUATION * coverage.powf(CLOUD_COVERAGE_EXPONENT)
}

/// The cloud coverage Kasten-Czeplak implies for an attenuation factor.
///
/// The NASA clearness path supplies an attenuation without a cloud number; inverting the same
/// expression keeps the two weather features describing one sky instead of contradicting each
/// other.
pub fn cloud_pct_from_attenuation(attenuation: f64) -> f64 {
    if !attenuation.is_finite() {
        return 0.0;
    }
    let factor = attenuation.clamp(1.0 - CLOUD_MAX_ATTENUATION, 1.0);
    100.0 * ((1.0 - factor) / CLOUD_MAX_ATTENUATION).powf(1.0 / CLOUD_COVERAGE_EXPONENT)
}
