//! Why a fleet read failed.

use thiserror::Error;

/// A fleet read that produced no usable view.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FleetError {
    /// The transport or the cache could not answer.
    #[error("fleet read failed: {reason}")]
    Unavailable {
        /// Human-readable cause.
        reason: String,
    },
}
