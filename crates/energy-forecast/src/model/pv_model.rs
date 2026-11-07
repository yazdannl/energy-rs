//! A loaded, validated model: the ensemble plus what it says about itself.

use std::path::Path;

use super::loader;
use super::metadata::ModelMetadata;
use crate::ForecastError;
use crate::features::Features;
use crate::gbt::Ensemble;

/// The served artifact. Load once per process and share it; it is immutable.
#[derive(Debug, Clone)]
pub struct PvModel {
    ensemble: Ensemble,
    metadata: ModelMetadata,
}

impl PvModel {
    /// Assemble a model from an already validated ensemble and its metadata.
    pub fn new(ensemble: Ensemble, metadata: ModelMetadata) -> Self {
        Self { ensemble, metadata }
    }

    /// Read and validate an artifact from disk.
    pub fn load_path(path: &Path) -> Result<Self, ForecastError> {
        loader::load_path(path)
    }

    /// Parse and validate an artifact from raw JSON bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ForecastError> {
        loader::parse(bytes)
    }

    /// The clamped multiplier on the physical estimate, for one feature vector.
    pub fn ratio(&self, features: &Features) -> Result<f64, ForecastError> {
        self.ensemble.ratio(features)
    }

    /// The raw ensemble sum, before the residual clamp.
    pub fn raw_ratio(&self, features: &Features) -> Result<f64, ForecastError> {
        self.ensemble.raw_ratio(features)
    }

    /// The residual clamp `(min, max)` this artifact declares.
    pub fn clamp(&self) -> (f64, f64) {
        self.ensemble.clamp()
    }

    /// Provenance and held-out accuracy.
    pub fn metadata(&self) -> &ModelMetadata {
        &self.metadata
    }

    /// Number of trees in the ensemble.
    pub fn tree_count(&self) -> usize {
        self.ensemble.tree_count()
    }

    /// Number of nodes across all trees.
    pub fn node_count(&self) -> usize {
        self.ensemble.node_count()
    }

    /// Number of leaves across all trees.
    pub fn leaf_count(&self) -> usize {
        self.ensemble.leaf_count()
    }
}
