//! One pack's override with its expiry.

use crate::dispatch::OverrideAction;
use crate::dispatch::override_source::OverrideSource;

/// A per-pack operator (or maintenance) command that expires.
///
/// The expiry is part of the override, not of the engine: an override that outlives its window is
/// the classic "the battery kept charging for a day" bug, so every read takes `now` and every
/// expired entry is simply not returned.
#[derive(Debug, Clone, PartialEq)]
pub struct ManualOverride {
    /// The pack it applies to.
    pub pack_id: String,
    /// What it asks for.
    pub action: OverrideAction,
    /// When it stops applying, UTC epoch seconds (inclusive until then).
    pub until: i64,
    /// Who asked for it.
    pub source: OverrideSource,
}

impl ManualOverride {
    /// A manual override.
    pub fn manual(pack_id: impl Into<String>, action: OverrideAction, until: i64) -> Self {
        Self {
            pack_id: pack_id.into(),
            action,
            until,
            source: OverrideSource::Manual,
        }
    }

    /// A maintenance-window override.
    pub fn maintenance(pack_id: impl Into<String>, action: OverrideAction, until: i64) -> Self {
        Self {
            pack_id: pack_id.into(),
            action,
            until,
            source: OverrideSource::Maintenance,
        }
    }

    /// Whether the override is still in force at `now`.
    pub fn active(&self, now: i64) -> bool {
        now <= self.until
    }
}
