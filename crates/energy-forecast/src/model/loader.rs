//! Loading and validating a committed artifact.

use std::path::Path;

use super::artifact::Artifact;
use super::contract::{LEARNED_KIND, validate_digest, validate_names};
use super::pv_model::PvModel;
use crate::ForecastError;
use crate::gbt::Ensemble;

/// Parse an artifact from raw JSON bytes, validating it before any tree can be served.
pub fn parse(bytes: &[u8]) -> Result<PvModel, ForecastError> {
    let artifact: Artifact = serde_json::from_slice(bytes)
        .map_err(|error| ForecastError::BadArtifact(error.to_string()))?;
    build(artifact)
}

/// Read and parse an artifact from disk.
pub fn load_path(path: &Path) -> Result<PvModel, ForecastError> {
    let bytes = std::fs::read(path)
        .map_err(|error| ForecastError::ModelUnavailable(format!("{}: {error}", path.display())))?;
    parse(&bytes)
}

fn build(artifact: Artifact) -> Result<PvModel, ForecastError> {
    if artifact.kind != LEARNED_KIND {
        return Err(ForecastError::BadArtifact(format!(
            "unsupported PV model kind: {:?}",
            artifact.kind
        )));
    }
    validate_names(&artifact.feature_names)?;
    validate_digest(&artifact.metadata)?;
    let ensemble = Ensemble::from_artifact(&artifact)?;
    Ok(PvModel::new(ensemble, artifact.metadata))
}
