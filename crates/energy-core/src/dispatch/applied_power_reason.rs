//! Why a dispatch row has no measured applied watts.

/// The measurement failure behind a row whose `applied_w` is unknown, never a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedPowerReason {
    /// The pack sample was missing or older than the freshness bound.
    SampleStale,
    /// A transport read failed, so no sample could be taken.
    ReadFailed,
}

impl AppliedPowerReason {
    /// Stable slug for the dispatch log.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SampleStale => "sample_stale",
            Self::ReadFailed => "read_failed",
        }
    }
}
