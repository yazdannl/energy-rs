//! Event severity.

use crate::error::LedgerError;

/// Journal level. Stored as text so a future level can be added by migration without renumbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl EventLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub(crate) fn from_key(key: &str) -> Result<Self, LedgerError> {
        match key {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            other => Err(LedgerError::Invalid(format!(
                "event level {other:?} is unknown"
            ))),
        }
    }
}
