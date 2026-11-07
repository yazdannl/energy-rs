//! One diagnosed row: what was checked, the verdict and the evidence.

use serde::Serialize;

use crate::diag::status::Status;

/// A single check result. `detail` is evidence, never a secret: the doctor redacts tokens before it
/// reaches this type.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Check {
    /// Short stable name (`ha transport`, `stats solar 5min/24h`, `battery AB2000-X`).
    pub name: String,
    pub status: Status,
    /// One line of evidence: values, coverage, units, ages.
    pub detail: String,
}

impl Check {
    pub fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(name, Status::Pass, detail)
    }

    pub fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(name, Status::Warn, detail)
    }

    pub fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(name, Status::Fail, detail)
    }

    fn new(name: impl Into<String>, status: Status, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
        }
    }
}
