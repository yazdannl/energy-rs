//! What a provider reports about itself, before the merge decides what to do with it.

/// What a provider reports about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceHealth {
    /// The provider is switched off.
    Inactive,
    /// The provider answered and produced a series.
    Ok,
    /// The provider answered with a stale or partial series.
    Degraded,
    /// The provider failed.
    Failed,
}
