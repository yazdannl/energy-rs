//! The on-disk artifact schema: bit patterns stay integers here and are decoded by the loader.

use serde::{Deserialize, Serialize};

use super::metadata::ModelMetadata;
use super::tree_artifact::TreeArtifact;

/// One committed `pv_base.json`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Artifact {
    /// Artifact kind; must equal [`super::LEARNED_KIND`].
    pub kind: String,
    /// Feature names in frozen order.
    pub feature_names: Vec<String>,
    /// IEEE-754 binary64 baseline, as its `u64` bit pattern.
    pub baseline_bits: u64,
    /// Residual clamp; the constants are the fallback when the key is absent.
    #[serde(default)]
    pub residual_clamp: Option<[f64; 2]>,
    /// The ensemble, in artifact order.
    pub trees: Vec<TreeArtifact>,
    /// Provenance and accuracy metadata.
    #[serde(flatten)]
    pub metadata: ModelMetadata,
}
