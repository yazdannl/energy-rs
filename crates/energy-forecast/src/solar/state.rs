//! Per-source state, and the one rule that classifies it.

use super::health::SourceHealth;

/// The state a source reports and the merge acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    /// The provider answered and produced a series.
    Ok,
    /// The provider answered with a stale or partial series.
    Degraded,
    /// The provider failed or produced nothing.
    Failed,
    /// The provider is switched off.
    Disabled,
}

impl SourceState {
    /// Classify one source.
    ///
    /// `failed` wins over a non-empty series only for the report: the merge still lets a non-empty
    /// series contribute even when its status is `failed`, because a degraded provider that did
    /// answer is better than dropping its numbers.
    pub fn classify(health: SourceHealth, has_series: bool) -> Self {
        match (health, has_series) {
            (SourceHealth::Inactive, _) => Self::Disabled,
            (SourceHealth::Ok, true) => Self::Ok,
            (SourceHealth::Degraded, true) => Self::Degraded,
            (SourceHealth::Failed | SourceHealth::Ok | SourceHealth::Degraded, _) => Self::Failed,
        }
    }

    /// The status string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
        }
    }
}
