//! Where the operator's policy comes from.

use crate::plan::Policy;

/// Where the operator's policy comes from (the settings, re-read every tick).
pub trait PolicySource: Send + Sync {
    /// The configured policy, without the measured cost basis.
    fn policy(&self) -> Policy;
}
