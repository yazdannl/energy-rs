//! Artifact provenance and held-out accuracy, kept beside the model for the status report.

use serde::{Deserialize, Serialize};

/// What the artifact says about how it was trained.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ModelMetadata {
    /// Training timestamp, ISO-8601 UTC.
    #[serde(default)]
    pub trained_at: Option<String>,
    /// Number of training rows.
    #[serde(default)]
    pub training_rows: Option<u64>,
    /// Number of distinct PV systems in the corpus.
    #[serde(default)]
    pub training_systems: Option<u64>,
    /// Training window length, days.
    #[serde(default)]
    pub training_window_days: Option<f64>,
    /// Corpus description.
    #[serde(default)]
    pub corpus: Option<String>,
    /// Corpus attribution/licence text.
    #[serde(default)]
    pub attribution: Option<String>,
    /// Digest of the feature contract, first 16 hex of `sha256` over the joined names.
    #[serde(default)]
    pub feature_digest: Option<String>,
    /// Held-out per-system MAE as a percentage of nameplate.
    #[serde(default)]
    pub held_out_system_mae_pct_nameplate: Option<f64>,
    /// Held-out per-year MAE as a percentage of nameplate.
    #[serde(default)]
    pub held_out_year_mae_pct_nameplate: Option<f64>,
}

impl ModelMetadata {
    /// Empty metadata, for tests and for an artifact that carries none.
    pub fn empty() -> Self {
        Self::default()
    }
}
