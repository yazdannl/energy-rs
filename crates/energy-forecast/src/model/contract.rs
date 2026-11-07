//! The frozen feature contract and the artifact-level guards that protect it.

use crate::ForecastError;
use crate::digest::sha256_hex;
use crate::features::{FEATURE_COUNT, FEATURE_NAMES};

use super::metadata::ModelMetadata;

/// The artifact's own declared kind. Refusing anything else is the cheap guard against serving a
/// file that merely looks like this one.
pub const LEARNED_KIND: &str = "hist_gradient_boosted_trees_exact_bits";

/// Lower residual clamp, used when the artifact does not declare one.
pub const RESIDUAL_MIN: f64 = 0.3;

/// Upper residual clamp, used when the artifact does not declare one.
pub const RESIDUAL_MAX: f64 = 1.8;

/// Validate the artifact's feature list against this build's frozen order.
pub fn validate_names(names: &[String]) -> Result<(), ForecastError> {
    let matches = names.len() == FEATURE_COUNT
        && names
            .iter()
            .zip(FEATURE_NAMES.iter())
            .all(|(actual, expected)| actual == expected);
    if matches {
        Ok(())
    } else {
        Err(ForecastError::FeatureContract(format!(
            "artifact has {} features, this build serves {FEATURE_COUNT}",
            names.len()
        )))
    }
}

/// `sha256` of the comma-joined feature names, first 16 hex characters.
pub fn feature_digest() -> String {
    let joined = FEATURE_NAMES.join(",");
    sha256_hex(joined.as_bytes()).chars().take(16).collect()
}

/// Check the artifact's recorded digest against this build's contract.
///
/// An artifact without a digest is trusted on its names alone (older artifacts predate the field);
/// an artifact whose digest disagrees is refused, because that means the artifact and this build
/// are not describing the same feature vector.
pub fn validate_digest(metadata: &ModelMetadata) -> Result<(), ForecastError> {
    let Some(recorded) = metadata.feature_digest.as_deref() else {
        return Ok(());
    };
    let expected = feature_digest();
    if recorded == expected {
        Ok(())
    } else {
        Err(ForecastError::FeatureContract(format!(
            "artifact feature digest {recorded} does not match this build's {expected}"
        )))
    }
}
