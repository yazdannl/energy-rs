//! RFC3339 UTC timestamps in and out.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde::ser::Serializer;

use crate::api::error::ApiError;

/// A UTC instant rendered as RFC3339 with a `Z` suffix and second resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// From UTC epoch seconds; an out-of-range value is an internal error, not a 400.
    pub(crate) fn from_epoch(secs: i64) -> Result<Self, ApiError> {
        DateTime::from_timestamp(secs, 0)
            .map(Self)
            .ok_or_else(|| ApiError::internal("timestamp", format!("epoch {secs} is out of range")))
    }

    /// From an already-parsed instant.
    pub(crate) fn from_datetime(at: DateTime<Utc>) -> Self {
        Self(at)
    }

    /// The current instant.
    pub(crate) fn now() -> Self {
        Self(Utc::now())
    }

    /// Epoch seconds.
    pub(crate) fn epoch(self) -> i64 {
        self.0.timestamp()
    }

    /// Convert an optional epoch, dropping an invalid value (stream events must not fail).
    pub(crate) fn lenient(secs: Option<i64>) -> Option<Self> {
        secs.and_then(|value| DateTime::from_timestamp(value, 0))
            .map(Self)
    }

    /// RFC3339 text.
    pub(crate) fn to_rfc3339(self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Secs, true)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

/// Parse an RFC3339 instant (any offset) into UTC; a bad value is a 400.
pub(crate) fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|at| at.with_timezone(&Utc))
        .map_err(|err| ApiError::query(format!("timestamp {value:?} is not RFC3339: {err}")))
}
