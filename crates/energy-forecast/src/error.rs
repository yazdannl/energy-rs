//! One error type for every refusal in this crate.

use thiserror::Error;

/// Everything the forecast crate refuses.
///
/// The crate never guesses: a missing artifact, a mismatched feature contract or an unreadable
/// learning payload is reported as a variant here and turned into a provider status by the caller.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ForecastError {
    /// The artifact is missing or cannot be read.
    #[error("forecast model unavailable: {0}")]
    ModelUnavailable(String),
    /// The artifact is present but is not a model this build can serve.
    #[error("invalid PV model artifact: {0}")]
    BadArtifact(String),
    /// The artifact's feature list is not the frozen contract.
    #[error("feature contract does not match this build: {0}")]
    FeatureContract(String),
    /// A learning payload was written by a schema this build does not read.
    #[error("learning payload schema v{found} is not supported (this build reads v{expected})")]
    SchemaVersion {
        /// Version found in the payload.
        found: u32,
        /// Version this build writes and reads.
        expected: u32,
    },
    /// A learning payload is malformed.
    #[error("learning payload is not usable: {0}")]
    Payload(String),
}
